//! Content-free, bounded evidence at the actual helper stdout boundary.
use super::*;
use atlas_agent::collaborator::BoardCall;
use std::fs::{self, OpenOptions};
pub(super) struct Call {
    pub request: String,
    pub binding: Value,
    pub id: String,
    pub canonical: &'static str,
}
impl Call {
    pub fn model(call: &BoardCall) -> Self {
        let canonical = match &call.operation {
            atlas_agent::collaborator::BoardOperation::Read { .. } => "slate_board_read",
            atlas_agent::collaborator::BoardOperation::Invoke { .. } => "slate_board_command",
            _ => "unavailable",
        };
        Self {
            request: call.request_id.clone(),
            binding: json!(call.session),
            id: call.id.clone(),
            canonical,
        }
    }
}
pub(super) struct Audit {
    path: PathBuf,
    count: usize,
}
struct Receipt<'a> {
    call: &'a Call,
    id: &'a Value,
    bytes: &'a [u8],
    ticket: Option<&'a str>,
}
impl Audit {
    pub(super) fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("helper-delivery.jsonl"),
            count: 0,
        }
    }
    fn record(
        &mut self,
        receipt: &Receipt<'_>,
        phase: &str,
        success: bool,
        current: bool,
    ) -> Result<(), String> {
        let call = receipt.call;
        let id = receipt.id;
        let bytes = receipt.bytes;
        let ticket = receipt.ticket;
        if self.count >= 64 || fs::metadata(&self.path).is_ok_and(|meta| meta.len() >= 65_536) {
            return Err("MCP delivery receipt budget exhausted".into());
        }
        let id = match id {
            Value::Number(_) => id.clone(),
            Value::String(s) if s.len() <= 128 && !s.contains(['\r', '\n']) => id.clone(),
            _ => Value::Null,
        };
        let value = json!({"request":call.request,"session":call.binding,"call":call.id,"mcp_id":id,
            "canonical_tool":call.canonical,"phase":phase,"success":success,"current_binding":current,
            "native_ticket":ticket.filter(|ticket|atlas_agent::collaborator::valid_id(ticket)),
            "result_bytes":bytes.len(),"result_sha256":format!("{:x}",Sha256::digest(bytes)),
            "at_unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64});
        let mut line = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        line.push(b'\n');
        if line.len() > 2048
            || fs::metadata(&self.path).is_ok_and(|meta| meta.len() + line.len() as u64 > 65_536)
        {
            return Err("MCP delivery receipt exceeds its byte limit".into());
        }
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut file| file.write_all(&line))
            .map_err(|e| e.to_string())?;
        self.count += 1;
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn send(
        &mut self,
        output: &mut impl Write,
        binding: (&Path, CloudGuard),
        call: &BoardCall,
        id: Value,
        result: Value,
        ticket: Option<&str>,
    ) -> Result<(), String> {
        let (dir, guard) = binding;
        self.send_dispatch(
            output,
            || transport::binding_matches(dir, &call.session, &call.request_id, Some(guard)),
            &Call::model(call),
            id,
            result,
            ticket,
        )
    }
    pub(super) fn send_dispatch(
        &mut self,
        output: &mut impl Write,
        current: impl Fn() -> bool,
        call: &Call,
        id: Value,
        result: Value,
        ticket: Option<&str>,
    ) -> Result<(), String> {
        let bytes = serde_json::to_vec(&result).map_err(|e| e.to_string())?;
        let success = result["isError"] != true;
        let receipt = Receipt {
            call,
            id: &id,
            bytes: &bytes,
            ticket,
        };
        self.record(&receipt, "output_prepared", success, current())?;
        if let Err(error) = super::send(
            output,
            json!({"jsonrpc":"2.0","id":id.clone(),"result":result}),
        ) {
            let _ = self.record(&receipt, "stdout_write_failed", false, current());
            return Err(error);
        }
        self.record(&receipt, "stdout_written_and_flushed", success, current())
    }
}
