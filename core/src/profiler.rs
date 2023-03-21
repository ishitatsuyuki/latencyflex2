use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::thread::sleep;

use chrono::Local;

use crate::{FrameId, Interval, MarkType, SectionId, Timestamp};

pub struct Profiler {
    output: BufWriter<File>,
    is_first_mark: bool,
}

impl Profiler {
    pub fn new() -> Profiler {
        let mut output = BufWriter::new(loop {
            let filename = format!("lfx2.{}.json", Local::now().format("%Y.%m.%d-%H.%M.%S"));
            let result = OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&filename);
            match result {
                Ok(f) => break f,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    sleep(std::time::Duration::from_secs(1));
                }
                Err(e) => panic!("Failed to open file {}: {}", filename, e),
            }
        });
        writeln!(output, "[").unwrap();
        Profiler {
            output,
            is_first_mark: true,
        }
    }

    pub fn mark(
        &mut self,
        frame_id: FrameId,
        section_id: SectionId,
        mark_type: MarkType,
        timestamp: Timestamp,
    ) {
        let name = frame_id.0;
        let tid = section_id;
        let ph = match mark_type {
            MarkType::Begin => "B",
            MarkType::End => "E",
        };
        let ts = timestamp / 1000;
        let comma = if self.is_first_mark { "" } else { ",\n" };
        self.is_first_mark = false;
        let _ = write!(
            self.output,
            r#"{comma}  {{"name": "{name}", "cat": "MARKER", "ph": "{ph}", "pid": 1, "tid": {tid}, "ts": {ts}}}"#
        );
    }

    pub fn latency(
        &mut self,
        frame_id: FrameId,
        latency: Interval,
        queueing_delay: Interval,
        finish_time: Timestamp,
    ) {
        let ts = finish_time / 1000;
        let comma = if self.is_first_mark { "" } else { ",\n" };
        self.is_first_mark = false;
        let _ = write!(
            self.output,
            r#"{comma}  {{"name": "Latency", "cat": "LATENCY", "ph": "C", "pid": 1, "tid": "10000", "ts": {ts}, "args": {{"latency": {latency}, "queueing_delay": {queueing_delay}}}}}"#
        );
    }

    pub fn frame_time(
        &mut self,
        frame_id: FrameId,
        top_interval: Interval,
        bop_interval: Interval,
        finish_time: Timestamp,
    ) {
        let name = frame_id.0;
        let ts = finish_time / 1000;
        let comma = if self.is_first_mark { "" } else { ",\n" };
        self.is_first_mark = false;
        let _ = write!(
            self.output,
            r#"{comma}  {{"name": "Frame Time", "cat": "LATENCY", "ph": "C", "pid": 1, "tid": "10000", "ts": {ts}, "args": {{"top_interval": {top_interval}, "bop_interval": {bop_interval}}}}}"#
        );
    }
    
    pub fn sleep(
        &mut self,
        frame_id: FrameId,
        start_time: Timestamp,
        end_time: Timestamp,
    ) {
        let name = "Sleep";
        let tid = 9999;
        let start = start_time / 1000;
        let end = end_time / 1000;
        let comma = if self.is_first_mark { "" } else { ",\n" };
        self.is_first_mark = false;
        let _ = write!(
            self.output,
            r#"{comma}  {{"name": "{name}", "cat": "MARKER", "ph": "B", "pid": 1, "tid": {tid}, "ts": {start}}},
              {{"name": "{name}", "cat": "MARKER", "ph": "E", "pid": 1, "tid": {tid}, "ts": {end}}}"#
        );
    }
}
