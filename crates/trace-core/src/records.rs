use crate::{ImportError, ImportLimits, error, views::SourceRef};

pub(crate) struct Record {
    pub bytes: Vec<u8>,
    pub source: SourceRef,
    pub terminated: bool,
}

#[derive(Clone)]
pub(crate) struct RecordBudget {
    pub bytes: usize,
    pub records: usize,
    pub limits: ImportLimits,
}

#[derive(Clone)]
pub(crate) struct RecordDecoder {
    pub file: String,
    pub line: usize,
    pub pending: Vec<u8>,
}

impl RecordDecoder {
    pub fn push(
        &mut self,
        mut bytes: &[u8],
        budget: &mut RecordBudget,
    ) -> Result<Vec<Record>, ImportError> {
        let mut output = Vec::new();
        while !bytes.is_empty() {
            let count = bytes
                .iter()
                .position(|b| *b == b'\n')
                .map_or(bytes.len(), |i| i + 1);
            let source = SourceRef {
                file: self.file.clone(),
                line: self.line + 1,
            };
            if count
                > budget
                    .limits
                    .record_bytes
                    .saturating_sub(self.pending.len())
            {
                return Err(error(
                    "record_byte_limit",
                    "A physical record exceeds the byte limit.",
                    Some(source),
                ));
            }
            if count > budget.limits.total_bytes.saturating_sub(budget.bytes) {
                return Err(error(
                    "input_byte_limit",
                    "The recording exceeds the total byte limit.",
                    Some(source),
                ));
            }
            self.pending.extend_from_slice(&bytes[..count]);
            budget.bytes += count;
            let terminated = bytes[count - 1] == b'\n';
            bytes = &bytes[count..];
            if terminated {
                output.push(self.complete(budget, true)?);
            }
        }
        Ok(output)
    }

    pub fn finish_offline(
        &mut self,
        budget: &mut RecordBudget,
    ) -> Result<Option<Record>, ImportError> {
        if self.pending.is_empty() {
            return Ok(None);
        }
        self.complete(budget, false).map(Some)
    }

    fn complete(
        &mut self,
        budget: &mut RecordBudget,
        terminated: bool,
    ) -> Result<Record, ImportError> {
        let source = SourceRef {
            file: self.file.clone(),
            line: self.line + 1,
        };
        if budget.records >= budget.limits.records {
            return Err(error(
                "record_count_limit",
                "The recording exceeds the physical record count limit.",
                Some(source),
            ));
        }
        self.line += 1;
        budget.records += 1;
        Ok(Record {
            bytes: std::mem::take(&mut self.pending),
            source,
            terminated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_eof_emits_nonempty_tail_once_and_empty_live_reads_do_not() {
        let mut decoder = RecordDecoder {
            file: "events.jsonl".into(),
            line: 0,
            pending: vec![],
        };
        let mut budget = RecordBudget {
            bytes: 0,
            records: 0,
            limits: ImportLimits::default(),
        };
        assert!(
            decoder
                .push(br#"{"type":"session_start"}"#, &mut budget)
                .unwrap()
                .is_empty()
        );
        assert!(decoder.push(b"", &mut budget).unwrap().is_empty());
        assert_eq!(budget.records, 0);
        let record = decoder.finish_offline(&mut budget).unwrap().unwrap();
        assert_eq!(record.source.line, 1);
        assert!(!record.terminated);
        assert_eq!(budget.records, 1);
        assert!(decoder.finish_offline(&mut budget).unwrap().is_none());
    }
    #[test]
    fn split_utf8_and_unterminated_json_wait_for_newline() {
        let mut decoder = RecordDecoder {
            file: "events.jsonl".into(),
            line: 0,
            pending: vec![],
        };
        let mut budget = RecordBudget {
            bytes: 0,
            records: 0,
            limits: ImportLimits::default(),
        };
        assert!(
            decoder
                .push(b"{\"text\":\"\xd0", &mut budget)
                .unwrap()
                .is_empty()
        );
        assert!(decoder.push(b"\xb0\"}", &mut budget).unwrap().is_empty());
        let records = decoder.push(b"\n", &mut budget).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].bytes, "{\"text\":\"а\"}\n".as_bytes());
        assert_eq!(records[0].source.line, 1);
        assert!(records[0].terminated);
        assert_eq!(budget.bytes, records[0].bytes.len());
        assert_eq!(budget.records, 1);
    }
}
