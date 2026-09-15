use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use tracing::warn;

use crate::audio::capture::{AudioCaptureSession, CapturedAudio};
use crate::stt::engine::{DecodeOptions, Transcriber, Transcript};
use crate::stt::BackendTranscriber;

const PARTIAL_WINDOW_MS: u64 = 15_000;
const PARTIAL_INTERVAL_MS: Duration = Duration::from_millis(900);
const MIN_PARTIAL_AUDIO_MS: u64 = 1_200;

pub struct SentenceStreamingTranscriber {
    request_tx: Sender<WorkerRequest>,
    result_rx: Receiver<WorkerResult>,
    worker_handle: Option<thread::JoinHandle<()>>,
    accumulator: SentenceAccumulator,
    last_snapshot_sent_at: Option<Instant>,
    last_committed_recording_ms: Option<u64>,
    pending_request: bool,
    disabled: bool,
}

impl SentenceStreamingTranscriber {
    pub fn new(transcriber: BackendTranscriber, options: DecodeOptions) -> Self {
        let (request_tx, request_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let worker_handle =
            thread::spawn(move || worker_main(request_rx, result_tx, transcriber, options));

        Self {
            request_tx,
            result_rx,
            worker_handle: Some(worker_handle),
            accumulator: SentenceAccumulator::default(),
            last_snapshot_sent_at: None,
            last_committed_recording_ms: None,
            pending_request: false,
            disabled: false,
        }
    }

    pub fn take_live_stream_delta(&mut self) -> Option<String> {
        self.accumulator.take_live_stream_delta()
    }

    pub fn tick(&mut self, session: &AudioCaptureSession) {
        self.collect_results();
        if self.disabled || self.pending_request {
            return;
        }

        let now = Instant::now();
        if self
            .last_snapshot_sent_at
            .is_some_and(|last| now.duration_since(last) < PARTIAL_INTERVAL_MS)
        {
            return;
        }

        let snapshot = match session.snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                warn!("failed to snapshot in-progress recording: {error:#}");
                self.disabled = true;
                return;
            }
        };
        if snapshot.duration_ms < MIN_PARTIAL_AUDIO_MS {
            return;
        }

        if self
            .request_tx
            .send(WorkerRequest::Decode(DecodeRequest {
                recorded_duration_ms: snapshot.duration_ms,
                captured: snapshot.tail_ms(PARTIAL_WINDOW_MS),
            }))
            .is_err()
        {
            warn!("partial transcription worker disconnected");
            self.disabled = true;
            return;
        }

        self.pending_request = true;
        self.last_snapshot_sent_at = Some(now);
    }

    pub fn finalize(
        mut self,
        captured: &CapturedAudio,
        transcriber: &impl Transcriber,
        options: &DecodeOptions,
    ) -> Result<Transcript> {
        self.collect_results();
        self.stop_worker();

        if !self.accumulator.has_committed_text()
            || self.needs_full_final_pass(captured.duration_ms)
        {
            return transcriber.transcribe(&captured.pcm_16khz_mono, options);
        }

        let tail = captured.tail_ms(PARTIAL_WINDOW_MS);
        let final_transcript = transcriber.transcribe(&tail.pcm_16khz_mono, options)?;
        let text = self.accumulator.finish(&final_transcript.text);
        Ok(Transcript {
            text,
            latency_ms: final_transcript.latency_ms,
        })
    }

    fn collect_results(&mut self) {
        loop {
            match self.result_rx.try_recv() {
                Ok(result) => {
                    self.pending_request = false;
                    match result.transcript {
                        Ok(transcript) => {
                            if self.accumulator.observe(&transcript.text) {
                                self.last_committed_recording_ms =
                                    Some(result.recorded_duration_ms);
                            }
                        }
                        Err(error) => warn!("partial transcription failed: {error:#}"),
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.pending_request = false;
                    self.disabled = true;
                    break;
                }
            }
        }
    }

    fn stop_worker(&mut self) {
        let _ = self.request_tx.send(WorkerRequest::Stop);
        let _ = self.worker_handle.take();
    }

    fn needs_full_final_pass(&self, captured_duration_ms: u64) -> bool {
        let Some(last_committed_recording_ms) = self.last_committed_recording_ms else {
            return false;
        };

        captured_duration_ms.saturating_sub(last_committed_recording_ms) > PARTIAL_WINDOW_MS
    }
}

impl Drop for SentenceStreamingTranscriber {
    fn drop(&mut self) {
        let _ = self.request_tx.send(WorkerRequest::Stop);
        let _ = self.worker_handle.take();
    }
}

enum WorkerRequest {
    Decode(DecodeRequest),
    Stop,
}

struct DecodeRequest {
    recorded_duration_ms: u64,
    captured: CapturedAudio,
}

struct WorkerResult {
    recorded_duration_ms: u64,
    transcript: Result<Transcript>,
}

fn worker_main(
    request_rx: Receiver<WorkerRequest>,
    result_tx: Sender<WorkerResult>,
    transcriber: BackendTranscriber,
    options: DecodeOptions,
) {
    while let Ok(request) = request_rx.recv() {
        match request {
            WorkerRequest::Decode(request) => {
                let transcript = transcriber.transcribe(&request.captured.pcm_16khz_mono, &options);
                if result_tx
                    .send(WorkerResult {
                        recorded_duration_ms: request.recorded_duration_ms,
                        transcript,
                    })
                    .is_err()
                {
                    break;
                }
            }
            WorkerRequest::Stop => break,
        }
    }
}

#[derive(Debug, Default)]
struct SentenceAccumulator {
    committed_text: String,
    previous_window_text: String,
    drained_live_prefix: String,
    pending_stream_delta: Option<String>,
}

impl SentenceAccumulator {
    fn observe(&mut self, transcript: &str) -> bool {
        let normalized = normalize_text(transcript);
        if normalized.is_empty() {
            self.previous_window_text.clear();
            return false;
        }

        let current_uncommitted = trim_committed_overlap(&self.committed_text, &normalized);
        let stable_prefix = common_word_prefix(&self.previous_window_text, &current_uncommitted);

        let mut sentence_committed = false;
        if let Some(committed) = extract_completed_sentence_prefix(&stable_prefix) {
            self.committed_text = join_text(&self.committed_text, &committed);
            self.previous_window_text = trim_committed_overlap(&self.committed_text, &normalized);
            sentence_committed = true;
        } else {
            self.previous_window_text = current_uncommitted;
        }

        self.queue_live_stream_delta(&stable_prefix);
        sentence_committed
    }

    fn take_live_stream_delta(&mut self) -> Option<String> {
        self.pending_stream_delta.take()
    }

    fn queue_live_stream_delta(&mut self, stable_prefix: &str) {
        let streamable = if stable_prefix.is_empty() {
            self.committed_text.clone()
        } else {
            join_text(
                &self.committed_text,
                &trim_committed_overlap(&self.committed_text, stable_prefix),
            )
        };
        if streamable.len() <= self.drained_live_prefix.len() {
            return;
        }
        if !streamable.starts_with(&self.drained_live_prefix) {
            return;
        }
        let delta = streamable[self.drained_live_prefix.len()..].trim_start();
        if delta.is_empty() {
            return;
        }
        self.drained_live_prefix.clone_from(&streamable);
        match &mut self.pending_stream_delta {
            Some(existing) => {
                if !existing.is_empty() {
                    existing.push(' ');
                }
                existing.push_str(delta);
            }
            None => self.pending_stream_delta = Some(delta.to_string()),
        }
    }

    fn finish(&self, transcript: &str) -> String {
        let normalized = normalize_text(transcript);
        let remainder = trim_committed_overlap(&self.committed_text, &normalized);
        join_text(&self.committed_text, &remainder)
    }

    fn has_committed_text(&self) -> bool {
        !self.committed_text.is_empty()
    }

}

pub fn remainder_after_typed(typed_prefix: &str, full_text: &str) -> String {
    trim_committed_overlap(typed_prefix, full_text)
}

fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn join_text(prefix: &str, suffix: &str) -> String {
    match (prefix.is_empty(), suffix.is_empty()) {
        (true, true) => String::new(),
        (true, false) => suffix.to_string(),
        (false, true) => prefix.to_string(),
        (false, false) => format!("{prefix} {suffix}"),
    }
}

fn trim_committed_overlap(committed: &str, text: &str) -> String {
    if committed.is_empty() || text.is_empty() {
        return text.to_string();
    }

    let committed_words: Vec<&str> = committed.split_whitespace().collect();
    let text_words: Vec<&str> = text.split_whitespace().collect();
    let max_overlap = committed_words.len().min(text_words.len());

    for overlap in (1..=max_overlap).rev() {
        if committed_words[committed_words.len() - overlap..] == text_words[..overlap] {
            return text_words[overlap..].join(" ");
        }
    }

    text.to_string()
}

fn common_word_prefix(left: &str, right: &str) -> String {
    let mut prefix = Vec::new();

    for (left_word, right_word) in left.split_whitespace().zip(right.split_whitespace()) {
        if left_word != right_word {
            break;
        }
        prefix.push(left_word);
    }

    prefix.join(" ")
}

fn extract_completed_sentence_prefix(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut last_sentence_end = None;

    for (index, word) in words.iter().enumerate() {
        if word_ends_sentence(word) {
            last_sentence_end = Some(index);
        }
    }

    last_sentence_end.map(|index| words[..=index].join(" "))
}

fn word_ends_sentence(word: &str) -> bool {
    let trimmed = word.trim_end_matches(|ch: char| matches!(ch, '"' | '\'' | ')' | ']' | '}'));
    trimmed
        .chars()
        .last()
        .is_some_and(|ch| matches!(ch, '.' | '!' | '?'))
}

#[cfg(test)]
mod tests {
    use super::{
        extract_completed_sentence_prefix, remainder_after_typed, SentenceAccumulator,
    };

    #[test]
    fn accumulator_commits_only_stable_sentence_prefixes() {
        let mut accumulator = SentenceAccumulator::default();

        accumulator.observe("hello there");
        assert!(!accumulator.has_committed_text());

        accumulator.observe("hello there.");
        assert!(!accumulator.has_committed_text());

        accumulator.observe("hello there. general kenobi");
        assert_eq!(accumulator.committed_text, "hello there.");

        accumulator.observe("hello there. general kenobi.");
        accumulator.observe("general kenobi. you are a bold one");
        assert_eq!(accumulator.committed_text, "hello there. general kenobi.");
    }

    #[test]
    fn accumulator_merges_final_tail_without_duplication() {
        let mut accumulator = SentenceAccumulator::default();

        accumulator.observe("first sentence.");
        accumulator.observe("first sentence. second sentence.");
        accumulator.observe("second sentence. third sentence is still changing");

        assert_eq!(
            accumulator.finish("second sentence. third sentence is done"),
            "first sentence. second sentence. third sentence is done"
        );
    }

    #[test]
    fn accumulator_streams_stable_word_prefixes_without_punctuation() {
        let mut accumulator = SentenceAccumulator::default();

        accumulator.observe("hello");
        assert!(accumulator.take_live_stream_delta().is_none());

        accumulator.observe("hello world");
        assert_eq!(
            accumulator.take_live_stream_delta(),
            Some("hello".to_string())
        );

        accumulator.observe("hello world today");
        assert_eq!(
            accumulator.take_live_stream_delta(),
            Some("world".to_string())
        );
        assert!(accumulator.take_live_stream_delta().is_none());
    }

    #[test]
    fn accumulator_streams_completed_sentences_as_they_stabilize() {
        let mut accumulator = SentenceAccumulator::default();

        accumulator.observe("hello there.");
        accumulator.observe("hello there. general kenobi");
        assert_eq!(
            accumulator.take_live_stream_delta(),
            Some("hello there.".to_string())
        );

        accumulator.observe("hello there. general kenobi.");
        accumulator.observe("general kenobi. you are a bold one");
        assert_eq!(
            accumulator.take_live_stream_delta(),
            Some("general kenobi.".to_string())
        );
        assert!(accumulator.take_live_stream_delta().is_none());
    }

    #[test]
    fn remainder_after_typed_excludes_already_streamed_prefix() {
        assert_eq!(
            remainder_after_typed(
                "first sentence. second sentence.",
                "first sentence. second sentence. third sentence"
            ),
            "third sentence"
        );
        assert_eq!(
            remainder_after_typed("", "first sentence. second sentence."),
            "first sentence. second sentence."
        );
    }

    #[test]
    fn completed_sentence_prefix_tracks_last_terminator() {
        assert_eq!(
            extract_completed_sentence_prefix("one two. three"),
            Some("one two.".to_string())
        );
        assert_eq!(
            extract_completed_sentence_prefix("one two! three four?"),
            Some("one two! three four?".to_string())
        );
        assert_eq!(extract_completed_sentence_prefix("one two three"), None);
    }
}
