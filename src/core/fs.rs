//! Cross-platform filesystem facade for XOS runtime data.
//!
//! Native builds delegate to the host filesystem. Browser builds use a small synchronous
//! virtual filesystem persisted in `localStorage`, which keeps Python app APIs synchronous.

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

const WASM_DATA_ROOT: &str = ".xos";

pub fn data_dir_string() -> Result<String, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::auth::auth_data_dir()
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| e.to_string())
    }
    #[cfg(target_arch = "wasm32")]
    {
        Ok(WASM_DATA_ROOT.to_string())
    }
}

pub fn exists(path: &str) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Path::new(path).exists()
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm::exists(path)
    }
}

pub fn is_dir(path: &str) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Path::new(path).is_dir()
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm::is_dir(path)
    }
}

pub fn create_dir_all(path: &str) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::create_dir_all(Path::new(path)).map_err(|e| format!("makedirs {path:?}: {e}"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm::create_dir_all(path)
    }
}

pub fn read(path: &str) -> Result<Vec<u8>, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read(Path::new(path)).map_err(|e| format!("read {path:?}: {e}"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm::read(path)
    }
}

pub fn read_to_string(path: &str) -> Result<String, String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read_to_string(Path::new(path)).map_err(|e| format!("read {path:?}: {e}"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm::read_to_string(path)
    }
}

pub fn write(path: &str, bytes: &[u8]) -> Result<(), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(parent) = Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {:?}: {e}", parent))?;
            }
        }
        std::fs::write(Path::new(path), bytes).map_err(|e| format!("write {path:?}: {e}"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm::write(path, bytes)
    }
}

pub fn write_string(path: &str, text: &str) -> Result<(), String> {
    write(path, text.as_bytes())
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    const KEY_PREFIX: &str = "xos.fs.v1";
    const KIND_FILE: &str = "file";
    const KIND_DIR: &str = "dir";

    fn normalize(path: &str) -> String {
        let replaced = path.replace('\\', "/");
        let mut parts = Vec::new();
        for part in replaced.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                p => parts.push(p),
            }
        }
        if parts.is_empty() {
            ".".to_string()
        } else {
            parts.join("/")
        }
    }

    fn key(kind: &str, path: &str) -> String {
        format!("{KEY_PREFIX}:{kind}:{}", normalize(path))
    }

    fn storage() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .ok_or_else(|| "xos.fs: browser window is unavailable".to_string())?
            .local_storage()
            .map_err(|e| format!("xos.fs: localStorage error: {e:?}"))?
            .ok_or_else(|| "xos.fs: localStorage is unavailable".to_string())
    }

    fn get_item(kind: &str, path: &str) -> Option<String> {
        storage().ok()?.get_item(&key(kind, path)).ok().flatten()
    }

    fn set_item(kind: &str, path: &str, value: &str) -> Result<(), String> {
        storage()?
            .set_item(&key(kind, path), value)
            .map_err(|e| format!("xos.fs: write localStorage failed: {e:?}"))
    }

    fn parent_dirs(path: &str) -> Vec<String> {
        let norm = normalize(path);
        let mut dirs = Vec::new();
        let mut parts: Vec<&str> = norm.split('/').collect();
        if parts.len() <= 1 {
            return dirs;
        }
        parts.pop();
        let mut current = String::new();
        for part in parts {
            if current.is_empty() {
                current.push_str(part);
            } else {
                current.push('/');
                current.push_str(part);
            }
            dirs.push(current.clone());
        }
        dirs
    }

    pub fn exists(path: &str) -> bool {
        get_item(KIND_FILE, path).is_some() || get_item(KIND_DIR, path).is_some()
    }

    pub fn is_dir(path: &str) -> bool {
        let norm = normalize(path);
        norm == "." || get_item(KIND_DIR, &norm).is_some()
    }

    pub fn create_dir_all(path: &str) -> Result<(), String> {
        let norm = normalize(path);
        if get_item(KIND_FILE, &norm).is_some() {
            return Err(format!(
                "cannot makedirs {path:?}: exists and is not a directory"
            ));
        }

        let mut current = String::new();
        for part in norm.split('/') {
            if part.is_empty() || part == "." {
                continue;
            }
            if current.is_empty() {
                current.push_str(part);
            } else {
                current.push('/');
                current.push_str(part);
            }
            set_item(KIND_DIR, &current, "1")?;
        }
        Ok(())
    }

    pub fn read(path: &str) -> Result<Vec<u8>, String> {
        read_to_string(path).map(|s| s.into_bytes())
    }

    pub fn read_to_string(path: &str) -> Result<String, String> {
        if let Some(value) = get_item(KIND_FILE, path) {
            return Ok(value);
        }
        let norm = normalize(path);
        let url = format!("/{norm}");
        fetch_text_blocking(&url)
    }

    pub fn write(path: &str, bytes: &[u8]) -> Result<(), String> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| format!("xos.fs: wasm storage currently expects UTF-8 for {path:?}"))?;
        for dir in parent_dirs(path) {
            create_dir_all(&dir)?;
        }
        set_item(KIND_FILE, path, text)
    }

    pub fn fetch_text_blocking(url: &str) -> Result<String, String> {
        let xhr =
            web_sys::XmlHttpRequest::new().map_err(|e| format!("xos.fs: XMLHttpRequest: {e:?}"))?;
        xhr.open_with_async("GET", url, false)
            .map_err(|e| format!("xos.fs: open {url}: {e:?}"))?;
        xhr.send()
            .map_err(|e| format!("xos.fs: request failed for {url}: {e:?}"))?;

        let status = xhr
            .status()
            .map_err(|e| format!("xos.fs: status for {url}: {e:?}"))?;
        if !(200..300).contains(&status) {
            return Err(format!("xos.fs: HTTP {status} for {url}"));
        }
        xhr.response_text()
            .map_err(|e| format!("xos.fs: response text for {url}: {e:?}"))?
            .ok_or_else(|| format!("xos.fs: empty response for {url}"))
    }
}

#[cfg(target_arch = "wasm32")]
pub fn download_to_path(url: &str, dest: &str) -> Result<(), String> {
    let text = wasm::fetch_text_blocking(url)?;
    write_string(dest, &text)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn download_to_path(url: &str, dest: &str) -> Result<(), String> {
    let path = Path::new(dest);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {:?}: {e}", parent))?;
        }
    }
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(64))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| format!("data.download: client: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("data.download: request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("data.download: HTTP {} for {url}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .map_err(|e| format!("data.download: body: {e}"))?;
    write(dest, &bytes)
}
