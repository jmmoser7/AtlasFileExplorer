use std::ffi::{c_void, CStr, OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::{env, mem};

use once_cell::sync::OnceCell;
use windows_sys::Win32::Foundation::{FreeLibrary, BOOL, HMODULE, PROC};
use windows_sys::Win32::Graphics::Gdi::HDC;
use windows_sys::Win32::Graphics::OpenGL::{self as gl, HGLRC, PIXELFORMATDESCRIPTOR};
use windows_sys::Win32::System::LibraryLoader as loader;

use crate::error::{ErrorKind, Result};

pub(super) const ATLAS_WGL_PROVIDER_DLL: &str = "ATLAS_WGL_PROVIDER_DLL";
const MESA_ICD: &str = "libgallium_wgl.dll";

/// One process owns both the WGL extension table and its originating provider.
static PROVIDER: OnceCell<Provider> = OnceCell::new();

#[derive(Debug)]
pub(super) struct Provider {
    library: HMODULE,
    mesa: bool,
    pub choose_pixel_format: unsafe extern "system" fn(HDC, *const PIXELFORMATDESCRIPTOR) -> i32,
    pub describe_pixel_format:
        unsafe extern "system" fn(HDC, i32, u32, *mut PIXELFORMATDESCRIPTOR) -> i32,
    pub set_pixel_format: unsafe extern "system" fn(HDC, i32, *const PIXELFORMATDESCRIPTOR) -> BOOL,
    pub swap_buffers: unsafe extern "system" fn(HDC) -> BOOL,
    pub create_context: unsafe extern "system" fn(HDC) -> HGLRC,
    pub delete_context: unsafe extern "system" fn(HGLRC) -> BOOL,
    pub get_current_context: unsafe extern "system" fn() -> HGLRC,
    pub get_current_dc: unsafe extern "system" fn() -> HDC,
    pub make_current: unsafe extern "system" fn(HDC, HGLRC) -> BOOL,
    pub share_lists: unsafe extern "system" fn(HGLRC, HGLRC) -> BOOL,
    wgl_get_proc_address: unsafe extern "system" fn(*const u8) -> PROC,
}

impl Provider {
    pub(super) fn selected() -> Result<&'static Self> {
        PROVIDER.get_or_try_init(|| {
            if let Some(path) = env::var_os(ATLAS_WGL_PROVIDER_DLL) {
                Self::mesa(Path::new(&path)).map_err(|error| error.with_wgl_step("provider load"))
            } else {
                Self::system()
            }
        })
    }

    fn system() -> Result<Self> {
        let name = env::var_os("GLUTIN_WGL_OPENGL_DLL").unwrap_or_else(|| "opengl32.dll".into());
        let name: Vec<_> = name.encode_wide().chain(Some(0)).collect();
        // SAFETY: name is a null-terminated Windows string.
        let library = unsafe { loader::LoadLibraryW(name.as_ptr()) };
        if library == 0 {
            return Err(ErrorKind::NotFound.into());
        }
        Ok(Self {
            library,
            mesa: false,
            choose_pixel_format: gl::ChoosePixelFormat,
            describe_pixel_format: gl::DescribePixelFormat,
            set_pixel_format: gl::SetPixelFormat,
            swap_buffers: gl::SwapBuffers,
            create_context: gl::wglCreateContext,
            delete_context: gl::wglDeleteContext,
            get_current_context: gl::wglGetCurrentContext,
            get_current_dc: gl::wglGetCurrentDC,
            make_current: gl::wglMakeCurrent,
            share_lists: gl::wglShareLists,
            wgl_get_proc_address: gl::wglGetProcAddress,
        })
    }

    fn mesa(path: &Path) -> Result<Self> {
        if !path.is_absolute() {
            return Err(ErrorKind::NotSupported("ATLAS_WGL_PROVIDER_DLL must be absolute").into());
        }
        // LoadLibraryExW requires native separators when using the full DLL path for
        // dependency resolution. Canonicalize both paths before passing them to Windows.
        let path = path.canonicalize()?;
        let expected_icd = path.with_file_name(MESA_ICD).canonicalize()?;
        Self::mesa_icd_loaded_from(&expected_icd)?;
        let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: name is null-terminated. The absolute DLL path controls the search for its
        // paired Mesa ICD; other dependencies may resolve from System32.
        let library = unsafe {
            loader::LoadLibraryExW(
                name.as_ptr(),
                0,
                loader::LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | loader::LOAD_LIBRARY_SEARCH_SYSTEM32,
            )
        };
        if library == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let provider = Self::mesa_icd_loaded_from(&expected_icd).and_then(|loaded| {
            if !loaded {
                return Err(ErrorKind::NotSupported("Mesa WGL did not load its paired ICD").into());
            }
            Self::mesa_exports(library)
        });
        if provider.is_err() {
            // SAFETY: No provider or callbacks survive a failed load.
            unsafe { FreeLibrary(library) };
        }
        provider
    }

    fn mesa_icd_loaded_from(expected: &Path) -> Result<bool> {
        let name: Vec<_> = OsStr::new(MESA_ICD).encode_wide().chain(Some(0)).collect();
        // SAFETY: name is null-terminated. A process-wide provider owns the loaded module.
        let module = unsafe { loader::GetModuleHandleW(name.as_ptr()) };
        if module == 0 {
            return Ok(false);
        }
        let mut path = vec![0; 32768];
        // SAFETY: path is writable for its supplied length.
        let len =
            unsafe { loader::GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32) }
                as usize;
        if len == 0 || len >= path.len() {
            return Err(std::io::Error::last_os_error().into());
        }
        let loaded = PathBuf::from(OsString::from_wide(&path[..len])).canonicalize()?;
        if loaded != expected {
            return Err(
                ErrorKind::NotSupported("Mesa WGL ICD loaded from a different directory").into(),
            );
        }
        Ok(true)
    }

    fn mesa_exports(library: HMODULE) -> Result<Self> {
        macro_rules! export {
            ($name:literal) => {{
                let name = concat!($name, "\0");
                // SAFETY: The Mesa module is loaded and the export name is null-terminated.
                let address = unsafe { loader::GetProcAddress(library, name.as_ptr()) }.ok_or(
                    ErrorKind::NotSupported(concat!("Mesa WGL is missing ", $name)),
                )?;
                // SAFETY: Each selected export has the exact Windows ABI of its named WGL
                // operation. The destination field specifies that operation's signature.
                unsafe { mem::transmute::<unsafe extern "system" fn() -> isize, _>(address) }
            }};
        }
        Ok(Self {
            library,
            mesa: true,
            choose_pixel_format: export!("wglChoosePixelFormat"),
            describe_pixel_format: export!("wglDescribePixelFormat"),
            set_pixel_format: export!("wglSetPixelFormat"),
            swap_buffers: export!("wglSwapBuffers"),
            create_context: export!("wglCreateContext"),
            delete_context: export!("wglDeleteContext"),
            get_current_context: export!("wglGetCurrentContext"),
            get_current_dc: export!("wglGetCurrentDC"),
            make_current: export!("wglMakeCurrent"),
            share_lists: export!("wglShareLists"),
            wgl_get_proc_address: export!("wglGetProcAddress"),
        })
    }

    pub(super) fn is_mesa(&self) -> bool {
        self.mesa
    }

    pub(super) fn get_proc_address(&self, name: &CStr) -> *const c_void {
        // SAFETY: name is null-terminated and the provider is kept loaded for the process.
        unsafe {
            let resolved = (self.wgl_get_proc_address)(name.as_ptr().cast())
                .map_or(std::ptr::null(), |function| function as *const c_void);
            if resolved.is_null() {
                loader::GetProcAddress(self.library, name.as_ptr().cast())
                    .map_or(std::ptr::null(), |function| function as *const c_void)
            } else {
                resolved
            }
        }
    }
}
