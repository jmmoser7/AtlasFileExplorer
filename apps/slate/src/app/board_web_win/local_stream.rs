//! A read-only WebView response body backed by the file handle checked by the caller.
use std::ffi::c_void;
use std::fs::File;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use windows::core::{implement, Ref, Result, HRESULT};
use windows::Win32::Foundation::{
    STG_E_ACCESSDENIED, STG_E_INVALIDFUNCTION, STG_E_INVALIDPOINTER, STG_E_MEDIUMFULL,
    STG_E_READFAULT, S_FALSE, S_OK,
};
use windows::Win32::System::Com::{
    ISequentialStream_Impl, IStream, IStream_Impl, LOCKTYPE, STATFLAG, STATSTG, STGC, STGM_READ,
    STGTY_STREAM, STREAM_SEEK, STREAM_SEEK_CUR, STREAM_SEEK_END, STREAM_SEEK_SET,
};

const CHUNK_BYTES: usize = 16 * 1024;

#[implement(IStream)]
struct FileStream {
    file: Arc<Mutex<File>>,
    position: Mutex<u64>,
    len: u64,
}

pub(super) fn from_file(file: File, len: u64) -> IStream {
    FileStream {
        file: Arc::new(Mutex::new(file)),
        position: Mutex::new(0),
        len,
    }
    .into()
}

impl FileStream {
    fn read_bytes(&self, buffer: &mut [u8]) -> Result<usize> {
        let mut pos = self.position.lock().unwrap_or_else(|err| err.into_inner());
        let wanted = self.len.saturating_sub(*pos).min(buffer.len() as u64) as usize;
        if wanted == 0 {
            return Ok(0);
        }
        let mut file = self.file.lock().unwrap_or_else(|err| err.into_inner());
        file.seek(SeekFrom::Start(*pos))
            .map_err(|_| STG_E_READFAULT)?;
        let mut count = 0;
        while count < wanted {
            match file.read(&mut buffer[count..wanted]) {
                Ok(0) => break,
                Ok(n) => {
                    count += n;
                    *pos += n as u64;
                }
                Err(err) if err.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return Err(STG_E_READFAULT.into()),
            }
        }
        Ok(count)
    }
}

impl ISequentialStream_Impl for FileStream_Impl {
    fn Read(&self, output: *mut c_void, size: u32, read: *mut u32) -> HRESULT {
        // COM callers may omit the byte count and may pass NULL for an empty read.
        if !read.is_null() {
            unsafe { read.write(0) };
        }
        if size == 0 {
            return S_OK;
        }
        if output.is_null() {
            return STG_E_INVALIDPOINTER;
        }
        // COM may supply uninitialized storage. Read through initialized Rust
        // memory, then copy only the bytes received into the caller's buffer.
        let mut scratch = [0; CHUNK_BYTES];
        let mut done = 0usize;
        while done < size as usize {
            let take = (size as usize - done).min(scratch.len());
            let n = match self.read_bytes(&mut scratch[..take]) {
                Ok(n) => n,
                Err(error) => return error.code(),
            };
            unsafe {
                std::ptr::copy_nonoverlapping(scratch.as_ptr(), output.cast::<u8>().add(done), n)
            };
            done += n;
            if !read.is_null() {
                unsafe { read.write(done as u32) };
            }
            if n < take {
                return S_FALSE;
            }
        }
        S_OK
    }

    fn Write(&self, _: *const c_void, _: u32, written: *mut u32) -> HRESULT {
        if !written.is_null() {
            unsafe { written.write(0) };
        }
        STG_E_ACCESSDENIED
    }
}

impl IStream_Impl for FileStream_Impl {
    fn Seek(&self, delta: i64, origin: STREAM_SEEK, output: *mut u64) -> Result<()> {
        let mut pos = self.position.lock().unwrap_or_else(|err| err.into_inner());
        let base = match origin {
            STREAM_SEEK_SET => 0,
            STREAM_SEEK_CUR => *pos,
            STREAM_SEEK_END => self.len,
            _ => return Err(STG_E_INVALIDFUNCTION.into()),
        };
        *pos = base
            .checked_add_signed(delta)
            .ok_or(STG_E_INVALIDFUNCTION)?;
        if !output.is_null() {
            unsafe { output.write(*pos) };
        }
        Ok(())
    }

    fn Stat(&self, output: *mut STATSTG, _: &STATFLAG) -> Result<()> {
        if output.is_null() {
            return Err(STG_E_INVALIDPOINTER.into());
        }
        unsafe {
            output.write(STATSTG {
                r#type: STGTY_STREAM.0 as u32,
                cbSize: self.len,
                grfMode: STGM_READ,
                ..Default::default()
            })
        };
        Ok(())
    }

    fn Clone(&self) -> Result<IStream> {
        let pos = *self.position.lock().unwrap_or_else(|err| err.into_inner());
        Ok(FileStream {
            file: self.file.clone(),
            position: Mutex::new(pos),
            len: self.len,
        }
        .into())
    }

    fn CopyTo(
        &self,
        dest: Ref<'_, IStream>,
        size: u64,
        read: *mut u64,
        written: *mut u64,
    ) -> Result<()> {
        let dest = dest.ok()?;
        let mut done = 0;
        let mut buffer = [0; CHUNK_BYTES];
        while done < size {
            let take = (size - done).min(buffer.len() as u64) as usize;
            let n = self.read_bytes(&mut buffer[..take])?;
            if n == 0 {
                break;
            }
            let mut accepted = 0;
            unsafe {
                dest.Write(buffer.as_ptr().cast(), n as u32, Some(&mut accepted))
                    .ok()?;
            }
            if accepted != n as u32 {
                return Err(STG_E_MEDIUMFULL.into());
            }
            done += n as u64;
        }
        if !read.is_null() {
            unsafe { read.write(done) };
        }
        if !written.is_null() {
            unsafe { written.write(done) };
        }
        Ok(())
    }

    fn SetSize(&self, _: u64) -> Result<()> {
        Err(STG_E_ACCESSDENIED.into())
    }
    fn Commit(&self, _: &STGC) -> Result<()> {
        Ok(())
    }
    fn Revert(&self) -> Result<()> {
        Err(STG_E_INVALIDFUNCTION.into())
    }
    fn LockRegion(&self, _: u64, _: u64, _: &LOCKTYPE) -> Result<()> {
        Err(STG_E_INVALIDFUNCTION.into())
    }
    fn UnlockRegion(&self, _: u64, _: u64, _: u32) -> Result<()> {
        Err(STG_E_INVALIDFUNCTION.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Com::{StructuredStorage::CreateStreamOnHGlobal, STATFLAG_NONAME};

    #[test]
    fn web_response_stream_reads_the_validated_handle_with_independent_seek_positions() {
        let dir =
            std::env::temp_dir().join(format!("slate-response-stream-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("asset.js");
        std::fs::write(&path, b"abcdef").unwrap();
        let stream = from_file(File::open(&path).unwrap(), 6);
        let original = dir.join("original.js");
        std::fs::rename(&path, &original).unwrap();
        std::fs::write(&path, b"replaced").unwrap();
        let read = |s: &IStream| {
            let mut b = [0u8; 3];
            let mut n = 0;
            let hr = unsafe { s.Read(b.as_mut_ptr().cast(), 3, Some(&mut n)) };
            (hr, b, n)
        };
        unsafe {
            let mut st = STATSTG::default();
            stream.Stat(&mut st, STATFLAG_NONAME).unwrap();
            assert_eq!((st.cbSize, st.grfMode), (6, STGM_READ));
            assert_eq!(read(&stream), (S_OK, *b"abc", 3));
            let independent = stream.Clone().unwrap();
            stream.Seek(-1, STREAM_SEEK_END, None).unwrap();
            assert_eq!(read(&stream), (S_FALSE, [b'f', 0, 0], 1));
            assert_eq!(read(&independent), (S_OK, *b"def", 3));
            assert_eq!(read(&stream), (S_FALSE, [0; 3], 0));
            assert!(stream.Seek(-7, STREAM_SEEK_CUR, None).is_err());
            stream.Seek(0, STREAM_SEEK_SET, None).unwrap();
            assert_eq!(
                stream.Write(b"x".as_ptr().cast(), 1, None),
                STG_E_ACCESSDENIED
            );
            assert!(stream.SetSize(1).is_err());
            let dest = CreateStreamOnHGlobal(Default::default(), true).unwrap();
            let (mut n, mut w) = (0, 0);
            stream
                .CopyTo(&dest, u64::MAX, Some(&mut n), Some(&mut w))
                .unwrap();
            assert_eq!((n, w), (6, 6));
            dest.Seek(0, STREAM_SEEK_SET, None).unwrap();
            assert_eq!(read(&dest), (S_OK, *b"abc", 3));
            assert_eq!(read(&dest), (S_OK, *b"def", 3));
            assert_eq!(stream.Read(std::ptr::null_mut(), 0, None), S_OK);
            assert_eq!(
                stream.Read(std::ptr::null_mut(), 1, None),
                STG_E_INVALIDPOINTER
            );
        }
        // A response may be larger than one transfer. Bytes appended after the
        // caller checked the length stay outside the advertised response.
        let contents: Vec<u8> = (0..CHUNK_BYTES * 3 + 7).map(|i| i as u8).collect();
        std::fs::write(&path, &contents).unwrap();
        let checked = contents.len() - 4;
        let bounded = from_file(File::open(&path).unwrap(), checked as u64);
        let mut body = vec![0xeeu8; contents.len()];
        let mut received = 0;
        assert_eq!(
            unsafe {
                bounded.Read(
                    body.as_mut_ptr().cast(),
                    body.len() as u32,
                    Some(&mut received),
                )
            },
            S_FALSE
        );
        assert_eq!(received as usize, checked);
        assert_eq!(&body[..checked], &contents[..checked]);
        assert_eq!(&body[checked..], &[0xee; 4]);
        drop(bounded);
        drop(stream);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
