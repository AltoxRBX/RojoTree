use serde_json::json;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use notify::{Watcher, RecursiveMode, Event};
use std::sync::mpsc::channel;
use std::time::Duration;
use colored::*;

fn to_posix(p: &Path) -> String {
    p.to_str().unwrap().replace('\\', "/")
}

fn to_pascal_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

struct VirtualPath {
    is_init: bool,
    target: String,
    folder: Vec<String>,
    name: String,
    file: String,
}

fn get_virtual_path(filepath: &Path, base_path: &Path) -> VirtualPath {
    let relative_path = filepath.strip_prefix(base_path).unwrap();
    let parts: Vec<String> = relative_path
        .iter()
        .map(|s| s.to_str().unwrap().to_string())
        .collect();
    
    let filename = filepath.file_stem().unwrap().to_str().unwrap();
    let is_server = filename.to_lowercase().contains("server");
    let folder_name = if parts.len() > 1 {
        to_pascal_case(&parts[parts.len() - 2])
    } else {
        String::new()
    };

    let name = if filename == "init" {
        folder_name.clone()
    } else if ["server", "client", "utils", "types"].contains(&filename.to_lowercase().as_str()) {
        format!("{}{}", folder_name, to_pascal_case(filename))
    } else {
        filename.to_string()
    };

    let file = if filename == "init" {
        let mut path_parts = vec!["src".to_string()];
        path_parts.extend(parts[..parts.len() - 1].iter().cloned());
        path_parts.join("/")
    } else {
        let mut path_parts = vec!["src".to_string()];
        path_parts.extend(parts.clone());
        path_parts.join("/")
    };

    let folder: Vec<String> = parts[..parts.len() - 1]
        .iter()
        .map(|s| to_pascal_case(s))
        .collect();

    VirtualPath {
        is_init: filename == "init",
        target: if is_server { "ServerScriptService" } else { "ReplicatedStorage" }.to_string(),
        folder,
        name,
        file,
    }
}

fn walk<F>(dir: &Path, blacklist: &[String], callback: &mut F)
where
    F: FnMut(&Path),
{
    let dir_posix = to_posix(dir);
    if blacklist.contains(&dir_posix) {
        return;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, blacklist, callback);
            } else if path.extension().and_then(|s| s.to_str()) == Some("luau") {
                callback(&path);
            }
        }
    }
}

fn generate_project_file() {
    let base_path = PathBuf::from("../../src");
    let blacklisted_dirs = vec![
        to_posix(&base_path.join("Start")),
        to_posix(&base_path.join("UI")),
    ];

    let mut init_claimed_folders = HashSet::new();

    let mut tree = json!({
        "name": "RojoTree",
        "tree": {
            "$className": "DataModel",
            "ReplicatedStorage": {
                "Shared": {
                    "$className": "Folder",
                "Classes": { "$path": "src/Others/Shared/Classes"},
                "Modules": { "$path": "src/Others/Shared/Modules"},
                },
                "Packages": { "$path": "Packages" },
                "UI": { "$path": "src/UI" }
            },
            "ServerScriptService": {
                "Initialize": { "$path": "src/Start/Server.server.luau" },
                "Packages": { "$path": "ServerPackages" },
                "Classes": { "$path": "src/Others/Server/Classes"},
                "Modules": { "$path": "src/Others/Server/Modules"},
            },
            "StarterPlayer": {
                "StarterPlayerScripts": {
                    "Initialize": { "$path": "src/Start/Client.client.luau" }
                },
            },
        }
    });

    let mut files_to_process = Vec::new();
    walk(&base_path, &blacklisted_dirs, &mut |filepath| {
        files_to_process.push(filepath.to_path_buf());
    });

    for filepath in &files_to_process {
        let vpath = get_virtual_path(filepath, &base_path);
        let full_folder_key = vpath.folder.join("/");

        if vpath.is_init {
            let root_key = if vpath.target == "ServerScriptService" {
                "ServerScriptService"
            } else {
                "ReplicatedStorage"
            };
            
            let root = tree["tree"][root_key].as_object_mut().unwrap();
            let parent_root = if vpath.target == "ServerScriptService" {
                root
            } else {
                root["Shared"].as_object_mut().unwrap()
            };

            let mut current = parent_root;
            for part in &vpath.folder[..vpath.folder.len() - 1] {
                if !current.contains_key(part) {
                    current.insert(part.clone(), json!({ "$className": "Folder" }));
                }
                current = current[part].as_object_mut().unwrap();
            }

            current.insert(vpath.name.clone(), json!({ "$path": vpath.file }));
            init_claimed_folders.insert(full_folder_key);
            continue;
        }

        if init_claimed_folders.contains(&full_folder_key) {
            continue;
        }

        let root_key = if vpath.target == "ServerScriptService" {
            "ServerScriptService"
        } else {
            "ReplicatedStorage"
        };
        
        let root = tree["tree"][root_key].as_object_mut().unwrap();
        let mut current = if vpath.target == "ServerScriptService" {
            root
        } else {
            root["Shared"].as_object_mut().unwrap()
        };

        for part in &vpath.folder {
            if !current.contains_key(part) {
                current.insert(part.clone(), json!({ "$className": "Folder" }));
            }
            current = current[part].as_object_mut().unwrap();
        }

        current.insert(vpath.name.clone(), json!({ "$path": vpath.file }));
    }

    fs::write(
        "../../default.project.json",
        serde_json::to_string_pretty(&tree).unwrap(),
    )
    .unwrap();

    println!("{}", "✅ default.project.json updated.".green().bold());
}

fn main() {
    println!("🚀 {} is running:", "RojoTree".purple().bold());
    generate_project_file();

    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, _>| {
        if let Ok(_) = res {
            let _ = tx.send(());
        }
    }).unwrap();

    watcher.watch(Path::new("../../src/Services"), RecursiveMode::Recursive).unwrap();

    loop {
        if rx.recv_timeout(Duration::from_millis(100)).is_ok() {
            std::thread::sleep(Duration::from_millis(200));
            generate_project_file();
        }
    }
}