use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::thread::sleep;

use chrono::Local;

use crate::{FrameId, MarkType, SectionId, Timestamp};

pub struct Profiler {
    output: BufWriter<File>,
    is_first_mark: bool,
}

impl Profiler {
    pub fn new() -> Profiler {
        let filename = format!("lfx2.{}.json", Local::now().format("%Y.%m.%d-%H.%M.%S"));
        let mut output = BufWriter::new(loop {
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
        let _ = self.output.flush();
    }
}
