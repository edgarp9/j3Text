fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=icon.ico");
    println!("cargo:rerun-if-env-changed=J3TEXT_SKIP_WINDOWS_RESOURCE");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("J3TEXT_SKIP_WINDOWS_RESOURCE").as_deref() != Ok("1")
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("icon.ico");
        resource.set_manifest(WINDOWS_MANIFEST);
        resource.compile()?;
    }

    Ok(())
}

const WINDOWS_MANIFEST: &str = r#"
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#;
