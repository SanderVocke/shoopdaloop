use common::logging::macros::*;
shoop_log_unit!("Frontend.FileIO");

use crate::cxx_qt_shoop::qobj_file_io_bridge::ffi::*;
pub use crate::cxx_qt_shoop::qobj_file_io_bridge::FileIORust;

use dunce;
use std::env;
use std::thread;
use std::time::Duration;

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_singleton_file_io(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

#[allow(unreachable_code)]
impl FileIO {
    pub fn wait_blocking(self: &FileIO, delay_ms: u64) {
        debug!("waiting for {} ms", delay_ms);
        thread::sleep(Duration::from_millis(delay_ms));
    }

    pub fn get_current_directory(self: &FileIO) -> QString {
        return match env::current_dir() {
            Ok(p) => match p.to_str() {
                Some(pp) => return QString::from(pp),
                _ => QString::default(),
            },
            Err(_) => {
                error!("failed to get current directory");
                QString::default()
            }
        };
    }

    pub fn write_file(self: &FileIO, file_name: QString, data: QString) -> bool {
        let file_name = file_name.to_string();
        let data = data.to_string();
        let result = std::fs::write(&file_name, data);
        match result {
            Ok(_) => {
                debug!("wrote file: {}", file_name);
                return true;
            }
            Err(_) => {
                error!("failed to write file: {}", file_name);
                return false;
            }
        }
    }

    pub fn read_file(self: &FileIO, file_name: QString) -> QString {
        let file_name = file_name.to_string();
        let result = std::fs::read_to_string(&file_name);
        match result {
            Ok(data) => {
                debug!("read file: {}", file_name);
                return QString::from(data.as_str());
            }
            Err(_) => {
                error!("failed to read file: {}", file_name);
                return QString::default();
            }
        }
    }

    pub fn create_temporary_file(self: &FileIO) -> QString {
        use tempfile::NamedTempFile;
        let file = NamedTempFile::new();
        match file {
            Ok(f) => {
                let path = f.path().to_owned();
                let ppath = path.to_str().unwrap();
                debug!("created temporary file: {}", ppath);
                let result = f.keep();
                return match result {
                    Ok(_) => QString::from(ppath),
                    Err(_) => QString::default(),
                };
            }
            Err(_) => {
                error!("failed to create temporary file");
                return QString::default();
            }
        }
    }

    pub fn generate_temporary_filename(self: &FileIO) -> QString {
        use tempfile::NamedTempFile;
        let temp_file = NamedTempFile::new();
        match temp_file {
            Ok(f) => {
                let path = f.path().to_owned();
                let ppath = path.to_str().unwrap();
                f.close().expect("Unable to close temporary file");
                debug!("generated temporary filename: {}", ppath);
                return QString::from(ppath);
            }
            Err(_) => {
                error!("failed to generate temporary filename");
                return QString::default();
            }
        }
    }

    pub fn create_temporary_folder(self: &FileIO) -> QString {
        use tempfile::tempdir;
        let temp_dir = tempdir();
        match temp_dir {
            Ok(d) => {
                let path = dunce::canonicalize(d.keep().to_owned()).unwrap();
                let ppath = path.to_str().unwrap();
                debug!("created temporary folder: {}", ppath);
                return QString::from(ppath);
            }
            Err(_) => {
                error!("failed to create temporary folder");
                return QString::default();
            }
        }
    }

    pub fn delete_recursive(self: &FileIO, path: QString) -> bool {
        use std::fs;
        use std::path::Path;
        let path = path.to_string();
        let path = Path::new(&path);
        if path.is_dir() {
            let result = fs::remove_dir_all(path);
            match result {
                Ok(_) => {
                    debug!("deleted dir recursive: {}", path.display());
                    return true;
                }
                Err(_) => {
                    error!("failed to delete dir recursive: {}", path.display());
                    return false;
                }
            }
        } else if path.is_file() {
            let result = fs::remove_file(path);
            match result {
                Ok(_) => {
                    debug!("deleted file: {}", path.display());
                    return true;
                }
                Err(_) => {
                    error!("failed to delete file: {}", path.display());
                    return false;
                }
            }
        } else {
            return false;
        }
    }

    pub fn delete_file(self: &FileIO, file_name: QString) -> bool {
        use std::fs;
        let file_name = file_name.to_string();
        let result = fs::remove_file(&file_name);
        match result {
            Ok(_) => {
                debug!("deleted file: {}", file_name);
                return true;
            }
            Err(_) => {
                error!("failed to delete file: {}", file_name);
                return false;
            }
        }
    }

    pub fn extract_tarfile(self: &FileIO, tarfile: QString, destination: QString) -> bool {
        use std::fs::File;
        use tar::Archive;
        let tarfile = tarfile.to_string();
        let destination = destination.to_string();
        let file = File::open(&tarfile);
        match file {
            Ok(f) => {
                let mut archive = Archive::new(f);
                let result = archive.unpack(&destination);
                match result {
                    Ok(_) => {
                        debug!("extracted tarfile: {} into {}", tarfile, destination);
                        return true;
                    }
                    Err(_) => {
                        error!(
                            "failed to extract tarfile: {} into {}",
                            tarfile, destination
                        );
                        return false;
                    }
                }
            }
            Err(_) => return false,
        }
    }

    pub fn make_tarfile(self: &FileIO, tarfile: QString, source: QString) -> bool {
        use std::fs::File;
        use tar::Builder;
        let tarfile = tarfile.to_string();
        let source = source.to_string();
        let file = File::create(&tarfile);
        match file {
            Ok(f) => {
                let mut archive = Builder::new(f);
                let result = archive.append_dir_all(".", &source);
                match result {
                    Ok(_) => {
                        debug!("created tarfile: {} from {}", tarfile, source);
                        return true;
                    }
                    Err(_) => {
                        error!("failed to create tarfile: {} from {}", tarfile, source);
                        return false;
                    }
                }
            }
            Err(_) => {
                error!("failed to create tarfile: {} from {}", tarfile, source);
                return false;
            }
        }
    }

    pub fn basename(self: &FileIO, path: QString) -> QString {
        use std::path::Path;
        let path = path.to_string();
        let path = Path::new(&path);
        let result = path.file_name();
        match result {
            Some(f) => return QString::from(f.to_str().unwrap()),
            _ => return QString::default(),
        }
    }

    pub fn is_absolute(self: &FileIO, path: QString) -> bool {
        use std::path::Path;
        let path = path.to_string();
        let path = Path::new(&path);
        return path.is_absolute();
    }

    pub fn realpath(self: &FileIO, path: QString) -> QString {
        let path = path.to_string();
        let result = dunce::canonicalize(&path);
        match result {
            Ok(p) => return QString::from(p.to_str().unwrap()),
            Err(_) => {
                error!("failed to get realpath of: {}", path);
                return QString::default();
            }
        }
    }

    pub fn exists(self: &FileIO, path: QString) -> bool {
        use std::path::Path;
        let p = path.to_string();
        let pp = Path::new(&p);
        return pp.exists();
    }

    pub fn glob(self: &FileIO, pattern: QString) -> QList_QString {
        use glob::glob;
        let pattern = pattern.to_string();
        let mut paths = QList_QString::default();
        let result = glob(&pattern);
        match result {
            Ok(g) => {
                for entry in g {
                    match entry {
                        Ok(p) => {
                            let p = p.to_str().unwrap();
                            paths.append(QString::from(p));
                        }
                        Err(_) => {}
                    }
                }
            }
            Err(_) => {}
        }
        return paths;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_wait_blocking() {
        use std::time::SystemTime;
        let obj = make_unique_fileio();
        let start = SystemTime::now();
        obj.wait_blocking(10);
        let end = SystemTime::now();
        let diff = end.duration_since(start).unwrap().as_millis();
        assert!(diff >= 10);
    }

    #[test]
    fn test_get_current_directory() {
        use std::env;
        let obj = make_unique_fileio();
        let prev = env::current_dir().unwrap();
        let dir = obj.create_temporary_folder();
        env::set_current_dir(dir.to_string()).unwrap();
        let current = obj.get_current_directory();
        env::set_current_dir(&prev).unwrap();
        assert_eq!(current.to_string(), dir.to_string());
    }

    #[test]
    fn test_delete_folder_and_exists() {
        let obj = make_unique_fileio();
        let name = obj.create_temporary_folder();
        assert_eq!(obj.exists(name.clone()), true);
        assert!(obj.delete_recursive(name.clone()));
        assert_eq!(obj.exists(name), false);
    }

    #[test]
    fn test_delete_file_and_exists() {
        let obj = make_unique_fileio();
        let filename = obj.create_temporary_file();
        assert!(obj.write_file(filename.clone(), QString::from("test")));
        assert_eq!(obj.exists(filename.clone()), true);
        assert!(obj.delete_file(filename.clone()));
        assert_eq!(obj.exists(filename), false);
    }

    #[test]
    fn test_write_read_file_roundtrip() {
        let obj = make_unique_fileio();
        let filename = obj.create_temporary_file();
        assert!(obj.write_file(filename.clone(), QString::from("test")));
        assert_eq!(obj.read_file(filename.clone()).to_string(), "test");
        assert!(obj.delete_file(filename));
    }

    #[test]
    fn test_make_and_extract_tarfile_roundtrip() {
        let obj = make_unique_fileio();
        let dir = obj.create_temporary_folder();
        let filename_a = format!("{dir}/a");
        let filename_b = format!("{dir}/b");
        assert!(obj.write_file(QString::from(&filename_a), QString::from("test_a")));
        assert!(obj.write_file(QString::from(&filename_b), QString::from("test_b")));
        let tarfile = obj.generate_temporary_filename();
        assert!(obj.make_tarfile(tarfile.clone(), dir.clone()));
        let destination = obj.create_temporary_folder();
        assert!(obj.extract_tarfile(tarfile.clone(), destination.clone()));
        let rfa = format!("{destination}/a");
        let rfb = format!("{destination}/b");
        assert_eq!(obj.read_file(QString::from(&rfa)).to_string(), "test_a");
        assert_eq!(obj.read_file(QString::from(&rfb)).to_string(), "test_b");
        assert!(obj.delete_recursive(dir));
        assert!(obj.delete_recursive(destination));
        assert!(obj.delete_file(tarfile));
    }

    #[test]
    fn test_basename() {
        assert_eq!(
            make_unique_fileio().basename(QString::from("/a/b/c")),
            QString::from("c")
        );
    }

    #[test]
    fn test_is_absolute_no() {
        assert_eq!(
            make_unique_fileio().is_absolute(QString::from("a/b/c")),
            false
        );
    }

    #[test]
    fn test_is_absolute_yes() {
        assert_eq!(
            make_unique_fileio().is_absolute(QString::from(if cfg!(windows) {
                "C:\\a\\b\\c"
            } else {
                "/a/b/c"
            })),
            true
        );
    }

    #[test]
    fn test_realpath() {
        let obj = make_unique_fileio();
        let folder = obj.create_temporary_folder();
        let base = obj.basename(folder.clone());
        let extended = format!("{folder}/../{base}");
        assert_eq!(
            obj.realpath(QString::from(&extended)).to_string(),
            folder.to_string()
        );
        assert!(obj.delete_recursive(folder));
    }

    #[test]
    fn test_glob() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("testfolder")).unwrap();
        std::fs::write(folder.path().join("test1"), "test").unwrap();
        std::fs::write(folder.path().join("test2"), "test").unwrap();
        std::fs::write(folder.path().join("testfolder").join("test1"), "test").unwrap();
        std::fs::write(folder.path().join("testfolder").join("test2"), "test").unwrap();

        let obj = make_unique_fileio();
        let qstring_to_pathbuf = |qs: &QString| {
            let str = qs.to_string();
            let mut pb = PathBuf::new();
            pb.push(str);
            pb
        };
        {
            let result = obj.glob(QString::from(&format!(
                "{folder}/*",
                folder = folder.path().to_str().unwrap()
            )));
            let paths: Vec<PathBuf> = result.iter().map(qstring_to_pathbuf).collect();
            let expect: Vec<PathBuf> = [
                folder.path().join("test1"),
                folder.path().join("test2"),
                folder.path().join("testfolder"),
            ]
            .to_vec();
            assert_eq!(paths, expect);
        }

        {
            let result = obj.glob(QString::from(&format!(
                "{folder}/**/test1",
                folder = folder.path().to_str().unwrap()
            )));
            let paths: Vec<PathBuf> = result.iter().map(qstring_to_pathbuf).collect();
            let expect: Vec<PathBuf> = [
                folder.path().join("test1"),
                folder.path().join("testfolder/test1"),
            ]
            .to_vec();
            assert_eq!(paths, expect);
        }
    }
}
