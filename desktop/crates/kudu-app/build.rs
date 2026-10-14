//! On Windows, embeds Kudu's icon, version information and application manifest in kudu.exe.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resources::embed();
    }
}

/// The build-dependencies for this (see Cargo.toml) and the resource compiler only exist when
/// building on Windows; cross-compiled builds go without the resources.
#[cfg(not(windows))]
mod windows_resources {
    pub fn embed() {}
}

#[cfg(windows)]
mod windows_resources {
    use std::fs::File;
    use std::io::BufWriter;
    use std::path::{Path, PathBuf};

    use image::imageops::{self, FilterType};
    use image::RgbaImage;

    /// The app icon (the glyph on its rounded square, so it shows on light and dark
    /// backgrounds alike; the bare glyph in assets/kudu-icon.png is white on transparent).
    const ICON: &str = "assets/app-icon.png";
    /// Sizes Explorer, the taskbar and Alt + Tab ask for at the common scale factors.
    const SIZES: [u32; 8] = [16, 20, 24, 32, 40, 48, 64, 256];

    /// Per-monitor DPI awareness (the overlay is placed in physical pixels), Windows 10+
    /// behaviour, long paths, and the modern common controls for the tray menu.
    const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
      <longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware>
    </windowsSettings>
  </application>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
        processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;

    pub fn embed() {
        let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
        let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
        let source = manifest_dir.join(ICON);
        println!("cargo:rerun-if-changed={}", source.display());

        let ico = out_dir.join("kudu.ico");
        write_icon(&source, &ico).unwrap_or_else(|error| panic!("generating {}: {error}", ico.display()));

        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon(ico.to_str().expect("OUT_DIR is valid UTF-8"))
            .set("FileDescription", "Kudu")
            .set("ProductName", "Kudu")
            .set("InternalName", "kudu")
            .set("OriginalFilename", "kudu.exe")
            .set("LegalCopyright", "Copyright (c) 2026 Manas Dutta. MIT License.")
            .set_manifest(MANIFEST);
        resource.compile_for("kudu").unwrap_or_else(|error| panic!("embedding the Windows resources: {error}"));
    }

    fn write_icon(source: &Path, destination: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let image = image::open(source)?.to_rgba8();
        let square = crop_to_opaque(&image);
        let mut icon = ico::IconDir::new(ico::ResourceType::Icon);
        for size in SIZES {
            let scaled = imageops::resize(&square, size, size, FilterType::Lanczos3);
            let entry = ico::IconImage::from_rgba_data(size, size, scaled.into_raw());
            // 32-bit BMP entries for the small sizes (read by everything), PNG for 256 px.
            let encoded = if size >= 256 {
                ico::IconDirEntry::encode_as_png(&entry)?
            } else {
                ico::IconDirEntry::encode_as_bmp(&entry)?
            };
            icon.add_entry(encoded);
        }
        icon.write(BufWriter::new(File::create(destination)?))?;
        Ok(())
    }

    /// The smallest square around the icon's solid part, dropping the transparent margin and
    /// faint shadow that macOS icons carry, so the icon fills its slot like other Windows icons.
    fn crop_to_opaque(image: &RgbaImage) -> RgbaImage {
        let (width, height) = image.dimensions();
        let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
        for (x, y, pixel) in image.enumerate_pixels() {
            if pixel[3] >= 200 {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x + 1);
                bottom = bottom.max(y + 1);
            }
        }
        if right <= left || bottom <= top {
            return image.clone();
        }
        let side = (right - left).max(bottom - top);
        let x = ((left + right) / 2).saturating_sub(side / 2).min(width.saturating_sub(side));
        let y = ((top + bottom) / 2).saturating_sub(side / 2).min(height.saturating_sub(side));
        imageops::crop_imm(image, x, y, side.min(width), side.min(height)).to_image()
    }
}
