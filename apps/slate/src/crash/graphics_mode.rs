use std::io;
use std::path::PathBuf;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIDevice, IDXGIFactory1};
use windows_core::Interface as _;

const PROVIDER: &str = "ATLAS_WGL_PROVIDER_DLL";
const OVERRIDE: &str = "ATLAS_SOFTWARE_GL";
const RESOURCE: &str = "runtime/software-gl/opengl32.dll";
const INTEL_VENDOR: u32 = 0x8086;
const IRIS_PLUS_DRIVER: [u16; 4] = [30, 0, 101, 3118];

mod d3d12;
pub(crate) use d3d12::graphics_probe;

fn packaged_provider() -> Option<PathBuf> {
    let path = std::env::current_exe().ok()?.parent()?.join(RESOURCE);
    path.is_file().then_some(path)
}

enum GraphicsMode {
    Automatic,
    Native,
    Software,
}

pub(super) fn prepare() -> io::Result<()> {
    let mut mode = match std::env::var(OVERRIDE).as_deref() {
        Ok("1") => GraphicsMode::Software,
        Ok("0") => GraphicsMode::Native,
        _ => GraphicsMode::Automatic,
    };
    for arg in std::env::args_os().skip(1) {
        match arg.to_str() {
            Some("--software-graphics") => mode = GraphicsMode::Software,
            Some("--native-graphics") => mode = GraphicsMode::Native,
            _ => {}
        }
    }

    // An updater restarted by the window inherits its environment. Only the
    // current launch's own selection may point at its packaged renderer.
    std::env::remove_var(PROVIDER);
    let selected = match mode {
        GraphicsMode::Native => {
            std::env::set_var(OVERRIDE, "0");
            return Ok(());
        }
        GraphicsMode::Automatic => packaged_provider().filter(|_| affected_graphics_adapter()),
        GraphicsMode::Software => Some(packaged_provider().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "This Slate installation is missing its software graphics runtime. Reinstall the Windows package.",
            )
        })?),
    };
    if let Some(provider) = selected {
        // The updater restarts Slate without the current command-line flags.
        if matches!(mode, GraphicsMode::Software) {
            std::env::set_var(OVERRIDE, "1");
        }
        let driver = if matches!(mode, GraphicsMode::Automatic) && d3d12::available(&provider) {
            "d3d12"
        } else {
            "llvmpipe"
        };
        std::env::set_var(PROVIDER, provider);
        std::env::set_var("GALLIUM_DRIVER", driver);
    }
    Ok(())
}

fn affected_graphics_adapter() -> bool {
    // DXGI enumerates the primary display adapter first. On WDDM 2.3 and
    // newer, the vendor's D3D and OpenGL UMDs share a driver-build version.
    // A compatibility default is limited to the Intel build seen hanging.
    let identity = unsafe {
        (|| {
            let factory = CreateDXGIFactory1::<IDXGIFactory1>().ok()?;
            let adapter = factory.EnumAdapters1(0).ok()?;
            let vendor = adapter.GetDesc1().ok()?.VendorId;
            let version = adapter.CheckInterfaceSupport(&IDXGIDevice::IID).ok()?;
            Some((vendor, driver_version(version)))
        })()
    };
    identity == Some((INTEL_VENDOR, IRIS_PLUS_DRIVER))
}

fn driver_version(value: i64) -> [u16; 4] {
    let bytes = value.to_be_bytes();
    std::array::from_fn(|index| u16::from_be_bytes([bytes[index * 2], bytes[index * 2 + 1]]))
}

#[cfg(test)]
mod tests {
    #[test]
    fn intel_build_is_decoded_from_dxgi_version() {
        assert_eq!(
            super::driver_version(0x001e_0000_0065_0c2e),
            super::IRIS_PLUS_DRIVER
        );
    }
}
