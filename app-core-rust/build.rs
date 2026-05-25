fn main() {
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set("FileDescription", "Autofix Engine");
        resource.set("ProductName", "Autofix");
        resource.set("OriginalFilename", "AF-BG-Engine.exe");
        resource.compile().expect("compile Windows resources");
    }
}
