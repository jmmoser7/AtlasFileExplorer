//! Best-effort native fault metadata. The supervising process records the exit
//! even when corruption, fail-fast, or a replaced Windows filter bypasses this.
//! No dumps, workbook contents, or full module paths are written here.

use std::ffi::c_void;
use std::fmt::{self, Write};
use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::IntoRawHandle;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
use windows::Win32::Storage::FileSystem::FILE_FLAG_WRITE_THROUGH;
use windows::Win32::System::Diagnostics::Debug::{
    SetUnhandledExceptionFilter, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS, EXCEPTION_RECORD,
};
use windows::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_IMAGE};
use windows::Win32::System::ProcessStatus::GetMappedFileNameW;
use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentThreadId};

static INSTALLED: AtomicBool = AtomicBool::new(false);
static HANDLING: AtomicBool = AtomicBool::new(false);
static REPORT_FILE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static PREVIOUS_FILTER: AtomicUsize = AtomicUsize::new(0);

// Raw calls avoid constructing a Windows error (and possibly allocating) when
// the process is already damaged. The synchronous handle was opened at startup.
#[link(name = "kernel32")]
unsafe extern "system" {
    fn WriteFile(
        file: *mut c_void,
        buffer: *const c_void,
        size: u32,
        written: *mut u32,
        overlapped: *const c_void,
    ) -> i32;
    fn FlushFileBuffers(file: *mut c_void) -> i32;
}

/// Install once, before starting app/renderer threads. `report_dir` already
/// exists locally. The deliberately leaked handle stays valid until exit;
/// successful sessions leave an empty file which the supervisor can remove.
pub(super) fn install(report_dir: &Path) -> io::Result<()> {
    if INSTALLED.swap(true, Ordering::AcqRel) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "native exception reporting already installed",
        ));
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(FILE_FLAG_WRITE_THROUGH.0)
        .open(report_dir.join("native-exception.txt"));
    let file = match file {
        Ok(file) => file,
        Err(error) => {
            INSTALLED.store(false, Ordering::Release);
            return Err(error);
        }
    };
    REPORT_FILE.store(file.into_raw_handle(), Ordering::Release);
    // SAFETY: our callback has the required Windows ABI and process lifetime.
    let previous = unsafe { SetUnhandledExceptionFilter(Some(exception_filter)) };
    PREVIOUS_FILTER.store(
        previous.map_or(0, |filter| filter as usize),
        Ordering::Release,
    );
    Ok(())
}

unsafe extern "system" fn exception_filter(info: *const EXCEPTION_POINTERS) -> i32 {
    if HANDLING.swap(true, Ordering::AcqRel) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    let file = REPORT_FILE.load(Ordering::Acquire);
    // SAFETY: Windows supplies these records for the duration of the callback.
    if let Some(record) = unsafe { info.as_ref().and_then(|info| info.ExceptionRecord.as_ref()) } {
        if !file.is_null() {
            write_exception(file, record);
        }
    }
    let previous = PREVIOUS_FILTER.load(Ordering::Acquire);
    let disposition = if previous == 0 {
        EXCEPTION_CONTINUE_SEARCH
    } else {
        // SAFETY: this address came from SetUnhandledExceptionFilter. Keeping
        // HANDLING set while chaining also prevents recursion in the old filter.
        let filter: unsafe extern "system" fn(*const EXCEPTION_POINTERS) -> i32 =
            unsafe { std::mem::transmute(previous) };
        unsafe { filter(info) }
    };
    HANDLING.store(false, Ordering::Release);
    disposition
}

fn write_exception(file: *mut c_void, record: &EXCEPTION_RECORD) {
    let mut module_path = [0u16; 512];
    let mut region = MEMORY_BASIC_INFORMATION::default();
    // Query mapped memory rather than walking the loader list: the faulting
    // thread may have died while it held the loader lock. These OS operations
    // remain best effort, as does all in-process reporting after a native fault.
    let found = unsafe {
        VirtualQuery(
            Some(record.ExceptionAddress),
            &mut region,
            std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
        )
    };
    let module_base =
        (found != 0 && region.Type == MEM_IMAGE).then_some(region.AllocationBase as usize);
    let path_len = if module_base.is_some() {
        (unsafe {
            GetMappedFileNameW(GetCurrentProcess(), region.AllocationBase, &mut module_path)
        }) as usize
    } else {
        0
    };
    // A truncated path cannot provide an honest basename.
    let module = if path_len != 0 && path_len < module_path.len() {
        module_basename(&module_path[..path_len])
    } else {
        &[]
    };
    let report =
        format_exception::<3072>(record, unsafe { GetCurrentThreadId() }, module, module_base);
    let mut written = 0;
    // SAFETY: the handle is process-lifetime, the buffer is live, and this file
    // uses synchronous I/O. There are no filesystem opens, Rust heap allocations,
    // session-log locks, or panicking formatting calls in the fault path.
    unsafe {
        WriteFile(
            file,
            report.bytes().as_ptr().cast(),
            report.bytes().len() as u32,
            &mut written,
            std::ptr::null(),
        );
        FlushFileBuffers(file);
    }
}

fn module_basename(path: &[u16]) -> &[u16] {
    let start = path
        .iter()
        .rposition(|&unit| unit == u16::from(b'\\') || unit == u16::from(b'/'))
        .map_or(0, |index| index + 1);
    &path[start..]
}

fn format_exception<const N: usize>(
    record: &EXCEPTION_RECORD,
    thread_id: u32,
    module: &[u16],
    module_base: Option<usize>,
) -> FixedText<N> {
    let mut report = FixedText::new();
    let address = record.ExceptionAddress as usize;
    let width = std::mem::size_of::<usize>() * 2;
    let code = record.ExceptionCode.0 as u32;
    let _ = writeln!(report, "exception_code=0x{code:08x}");
    let _ = writeln!(report, "exception_address=0x{address:0width$x}");
    let _ = writeln!(report, "exception_flags=0x{:08x}", record.ExceptionFlags);
    let _ = writeln!(report, "thread_id={thread_id}");
    let _ = report.write_str("module=");
    if module.is_empty() {
        let _ = report.write_str("<unknown>");
    } else {
        for decoded in char::decode_utf16(module.iter().copied()) {
            let character = decoded.unwrap_or(char::REPLACEMENT_CHARACTER);
            let _ = report.write_char(if character.is_control() {
                '?'
            } else {
                character
            });
        }
    }
    let _ = report.write_char('\n');
    if let Some(base) = module_base {
        let _ = writeln!(report, "module_base=0x{base:0width$x}");
        if let Some(offset) = address.checked_sub(base) {
            let _ = writeln!(report, "module_offset=0x{offset:x}");
        }
    }
    // Access violations and in-page errors supply the attempted operation and
    // memory address, not the contents at that address.
    if matches!(code, 0xc0000005 | 0xc0000006) && record.NumberParameters >= 2 {
        let access = match record.ExceptionInformation[0] {
            0 => "read",
            1 => "write",
            8 => "execute",
            _ => "unknown",
        };
        let _ = writeln!(report, "access={access}");
        let _ = writeln!(
            report,
            "memory_address=0x{:0width$x}",
            record.ExceptionInformation[1]
        );
    }
    report
}

struct FixedText<const N: usize> {
    buffer: [u8; N],
    len: usize,
}

impl<const N: usize> FixedText<N> {
    fn new() -> Self {
        Self {
            buffer: [0; N],
            len: 0,
        }
    }

    fn bytes(&self) -> &[u8] {
        &self.buffer[..self.len]
    }
}

impl<const N: usize> fmt::Write for FixedText<N> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let mut count = value.len().min(N - self.len);
        while !value.is_char_boundary(count) {
            count -= 1;
        }
        self.buffer[self.len..self.len + count].copy_from_slice(&value.as_bytes()[..count]);
        self.len += count;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::NTSTATUS;

    #[test]
    fn records_module_relative_location_and_access_violation_without_paths() {
        let record = EXCEPTION_RECORD {
            ExceptionCode: NTSTATUS(0xc0000005u32 as i32),
            ExceptionAddress: 0x1234usize as *mut c_void,
            NumberParameters: 2,
            ExceptionInformation: [1, 0x5678, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            ..Default::default()
        };
        let path: Vec<u16> = "\\Device\\private\\driver.dll".encode_utf16().collect();
        let report = format_exception::<1024>(&record, 42, module_basename(&path), Some(0x1000));
        let text = std::str::from_utf8(report.bytes()).unwrap();
        assert!(text.contains("exception_code=0xc0000005\n"));
        assert!(text.contains("thread_id=42\nmodule=driver.dll\n"));
        assert!(text.contains("module_offset=0x234\n"));
        assert!(text.contains("access=write\n"));
        assert!(text.contains("5678\n"));
        assert!(!text.contains("private"));
    }

    #[test]
    fn fixed_buffer_truncates_safely_and_preserves_utf8() {
        let mut report = FixedText::<5>::new();
        write!(report, "a🦀z").unwrap();
        write!(report, "ignored").unwrap();
        assert_eq!(std::str::from_utf8(report.bytes()).unwrap(), "a🦀");
        let mut empty = FixedText::<0>::new();
        empty.write_str("🦀").unwrap();
        assert!(empty.bytes().is_empty());
    }
}
