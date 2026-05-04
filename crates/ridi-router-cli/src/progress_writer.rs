use std::{
    collections::{hash_map::Entry, HashMap},
    fs::File,
    io::{self, BufWriter, Write},
    path::PathBuf,
    sync::Mutex,
};

use ridi_router_routing::{RoutingProgressEnvelope, RoutingProgressSink};
use tracing::error;

use crate::cli::output_dir::{prepare_empty_output_dir, OutputDirError};

#[derive(Debug, thiserror::Error)]
pub enum ProgressWriterError {
    #[error(transparent)]
    Directory { error: OutputDirError },

    #[error("Progress writer failed: {message}")]
    Runtime { message: String },
}

pub struct JsonlProgressSink {
    progress_dir: PathBuf,
    writers: Mutex<HashMap<u64, BufWriter<File>>>,
    first_error: Mutex<Option<String>>,
}

impl JsonlProgressSink {
    pub fn new(progress_dir: PathBuf) -> Result<Self, ProgressWriterError> {
        prepare_empty_output_dir(&progress_dir)
            .map_err(|error| ProgressWriterError::Directory { error })?;
        Ok(Self {
            progress_dir,
            writers: Mutex::new(HashMap::new()),
            first_error: Mutex::new(None),
        })
    }

    fn path_for_itinerary(&self, itinerary_id: u64) -> PathBuf {
        self.progress_dir.join(format!("{itinerary_id:06}.jsonl"))
    }

    fn write_envelope(&self, envelope: &RoutingProgressEnvelope) -> io::Result<()> {
        let Some(itinerary_id) = envelope.itinerary_id else {
            return Ok(());
        };

        let mut writers = self
            .writers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let writer = match writers.entry(itinerary_id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let file = File::create(self.path_for_itinerary(itinerary_id))?;
                entry.insert(BufWriter::new(file))
            }
        };
        serde_json::to_writer(&mut *writer, envelope)?;
        writer.write_all(b"\n")?;
        Ok(())
    }

    fn record_error(&self, error: &io::Error) {
        let mut first_error = self
            .first_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if first_error.is_none() {
            *first_error = Some(error.to_string());
        }
    }

    fn first_error(&self) -> Option<String> {
        self.first_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn flush_all(&self) -> io::Result<()> {
        let mut writers = self
            .writers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for writer in writers.values_mut() {
            writer.flush()?;
        }
        Ok(())
    }

    pub fn finish(&self) -> Result<(), ProgressWriterError> {
        if let Err(error) = self.flush_all() {
            self.record_error(&error);
        }

        if let Some(message) = self.first_error() {
            Err(ProgressWriterError::Runtime { message })
        } else {
            Ok(())
        }
    }
}

impl RoutingProgressSink for JsonlProgressSink {
    fn emit(&self, envelope: RoutingProgressEnvelope) {
        if let Err(write_error) = self.write_envelope(&envelope) {
            self.record_error(&write_error);
            error!(error = %write_error, "Failed to write routing progress event");
        }
    }
}

impl Drop for JsonlProgressSink {
    fn drop(&mut self) {
        if let Ok(writers) = self.writers.get_mut() {
            for writer in writers.values_mut() {
                if let Err(error) = writer.flush() {
                    error!(error = %error, "Failed to flush routing progress writer during drop");
                }
            }
        }
    }
}

#[allow(dead_code)]
fn _assert_send_sync<T: Send + Sync>() {}
#[allow(dead_code)]
fn _assert_jsonl_progress_sink_send_sync() {
    _assert_send_sync::<JsonlProgressSink>();
}
