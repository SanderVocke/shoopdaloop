use log::*;

#[cxx_qt::bridge]
pub mod qobj_file_io {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qlist.h");
        type QList_QString = cxx_qt_lib::QList<QString>;
    }

    unsafe extern "RustQt" {
        #[qobject]
        type FileIO = super::FileIORust;
    }

    unsafe extern "RustQt" {
        #[qinvokable]
        fn wait_blocking(self : &FileIO, delay_ms : u64);

        #[qinvokable]
        fn get_current_directory(self : &FileIO) -> QString;

        #[qinvokable]
        fn write_file(self : &FileIO, file_name : QString, data : QString) -> bool;

        #[qinvokable]
        fn read_file(self : &FileIO, file_name : QString) -> QString;

        #[qinvokable]
        fn create_temporary_file(self : &FileIO) -> QString;

        #[qinvokable]
        fn generate_temporary_filename(self : &FileIO) -> QString;

        #[qinvokable]
        fn create_temporary_folder(self : &FileIO) -> QString;

        #[qinvokable]
        fn delete_recursive(self : &FileIO, path : QString) -> bool;

        #[qinvokable]
        fn delete_file(self : &FileIO, file_name : QString) -> bool;

        #[qinvokable]
        fn extract_tarfile(self : &FileIO, tarfile : QString, destination : QString) -> bool;

        #[qinvokable]
        fn make_tarfile(self : &FileIO, tarfile : QString, source : QString) -> bool;

        #[qinvokable]
        fn basename(self : &FileIO, path : QString) -> QString;

        #[qinvokable]
        fn is_absolute(self : &FileIO, path : QString) -> bool;

        #[qinvokable]
        fn realpath(self : &FileIO, path : QString) -> QString;

        #[qinvokable]
        fn exists(self : &FileIO, path : QString) -> bool;

        #[qinvokable]
        fn glob(self : &FileIO, pattern : QString) -> QList_QString;
    }

    unsafe extern "C++" {
        include!("cxx-shoop/make_unique.h");

        #[rust_name = "make_unique_fileio"]
        fn make_unique() -> UniquePtr<FileIO>;
    }
}

#[derive(Default)]
pub struct FileIORust {}


use qobj_file_io::*;
use std::thread;
use std::time::Duration;
use std::env;

#[allow(unreachable_code)]
impl FileIO {
    pub fn wait_blocking(self : &FileIO, delay_ms : u64) {
        debug!("waiting for {} ms", delay_ms);
        thread::sleep(Duration::from_millis(delay_ms));
    }

    pub fn get_current_directory(self : &FileIO) -> QString {
        return match env::current_dir() {
            Ok(p) => match p.to_str() {
                    Some (pp) => return QString::from(pp),
                    _ => QString::default(),
                },
            Err(_) => {
                error!("failed to get current directory");
                QString::default()
            }
        }
    }

    pub fn write_file(self : &FileIO, file_name : QString, data : QString) -> bool {
        let file_name = file_name.to_string();
        let data = data.to_string();
        let result = std::fs::write(&file_name, data);
        match result {
            Ok(_) => {
                debug!("wrote file: {}", file_name);
                return true;
            },
            Err(_) => {
                error!("failed to write file: {}", file_name);
                return false;
            },
        }
    }

    pub fn read_file(self : &FileIO, file_name : QString) -> QString {
        let file_name = file_name.to_string();
        let result = std::fs::read_to_string(&file_name);
        match result {
            Ok(data) => {
                debug!("read file: {}", file_name);
                return QString::from(data.as_str());
            },
            Err(_) => {
                error!("failed to read file: {}", file_name);
                return QString::default();
            }
        }
    }

    pub fn create_temporary_file(self : &FileIO) -> QString {
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
                }
            },
            Err(_) => {
                error!("failed to create temporary file");
                return QString::default();
            }
        }
    }

    pub fn generate_temporary_filename(self : &FileIO) -> QString {
        use tempfile::NamedTempFile;
        let temp_file = NamedTempFile::new();
        match temp_file {
            Ok(f) => {
                let path = f.path().to_owned();
                let ppath = path.to_str().unwrap();
                f.close().expect("Unable to close temporary file");
                debug!("generated temporary filename: {}", ppath);
                return QString::from(ppath);
            },
            Err(_) => {
                error!("failed to generate temporary filename");
                return QString::default();
            }
        }
    }

    pub fn create_temporary_folder(self : &FileIO) -> QString {
        use tempfile::tempdir;
        let temp_dir = tempdir();
        match temp_dir {
            Ok(d) => {
                let path = d.into_path().to_owned();
                let ppath = path.to_str().unwrap();
                debug!("created temporary folder: {}", ppath);
                return QString::from(ppath);
            },
            Err(_) => {
                error!("failed to create temporary folder");
                return QString::default();
            }
        }
    }

    pub fn delete_recursive(self : &FileIO, path : QString) -> bool {
        use std::path::Path;
        use std::fs;
        let path = path.to_string();
        let path = Path::new(&path);
        if path.is_dir() {
            let result = fs::remove_dir_all(path);
            match result {
                Ok(_) => {
                    debug!("deleted dir recursive: {}", path.display());
                    return true;
                },
                Err(_) => {
                    error!("failed to delete dir recursive: {}", path.display());
                    return false;
                },
            }
        } else if path.is_file() {
            let result = fs::remove_file(path);
            match result {
                Ok(_) => {
                    debug!("deleted file: {}", path.display());
                    return true;
                },
                Err(_) => {
                    error!("failed to delete file: {}", path.display());
                    return false;
                },
            }
        } else {
            return false;
        }
    }

    pub fn delete_file(self : &FileIO, file_name : QString) -> bool {
        use std::fs;
        let file_name = file_name.to_string();
        let result = fs::remove_file(&file_name);
        match result {
            Ok(_) => {
                debug!("deleted file: {}", file_name);
                return true;
            },
            Err(_) => {
                error!("failed to delete file: {}", file_name);
                return false;
            },
        }
    }

    pub fn extract_tarfile(self : &FileIO, tarfile : QString, destination : QString) -> bool {
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
                    },
                    Err(_) => {
                        error!("failed to extract tarfile: {} into {}", tarfile, destination);
                        return false;
                    },
                }
            },
            Err(_) => return false,
        }
    }

    pub fn make_tarfile(self : &FileIO, tarfile : QString, source : QString) -> bool {
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
                    },
                    Err(_) => {
                        error!("failed to create tarfile: {} from {}", tarfile, source);
                        return false;
                    },
                }
            },
            Err(_) => {
                error!("failed to create tarfile: {} from {}", tarfile, source);
                return false;
            },
        }
    }

    pub fn basename(self : &FileIO, path : QString) -> QString {
        use std::path::Path;
        let path = path.to_string();
        let path = Path::new(&path);
        let result = path.file_name();
        match result {
            Some(f) => return QString::from(f.to_str().unwrap()),
            _ => return QString::default(),
        }
    }

    pub fn is_absolute(self : &FileIO, path : QString) -> bool {
        use std::path::Path;
        let path = path.to_string();
        let path = Path::new(&path);
        return path.is_absolute();
    }

    pub fn realpath(self : &FileIO, path : QString) -> QString {
        use std::fs;
        let path = path.to_string();
        let result = fs::canonicalize(&path);
        match result {
            Ok(p) => return QString::from(p.to_str().unwrap()),
            Err(_) => {
                error!("failed to get realpath of: {}", path);
                return QString::default();
            },
        }
    }

    pub fn exists(self : &FileIO, path : QString) -> bool {
        use std::path::Path;
        let p = path.to_string();
        let pp = Path::new(&p);
        return pp.exists();
    }

    pub fn glob(self : &FileIO, pattern : QString) -> QList_QString {
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
                        },
                        Err(_) => {},
                    }
                }
            },
            Err(_) => {},
        }
        return paths;
    }
}


#[cfg(test)]
mod tests {
    use super::*;

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
        panic!("Not implemented");
    }

    #[test]
    fn test_write_file() {
        panic!("Not implemented");
    }

    #[test]
    fn test_read_file() {
        panic!("Not implemented");
    }

    #[test]
    fn test_create_temporary_file() {
        panic!("Not implemented");
    }

    #[test]
    fn test_generate_temporary_filename() {
        panic!("Not implemented");
    }

    #[test]
    fn test_create_temporary_folder() {
        panic!("Not implemented");
    }

    #[test]
    fn test_delete_recursive() {
        panic!("Not implemented");
    }

    #[test]
    fn test_delete_file() {
        panic!("Not implemented");
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
        assert!(obj.make_tarfile(tarfile.clone(), dir));
        let destination = obj.create_temporary_folder();
        assert!(obj.extract_tarfile(tarfile, destination.clone()));
        let rfa = format!("{destination.clone()}/a");
        let rfb = format!("{destination}/b");
        assert_eq!(obj.read_file(QString::from(&rfa)).to_string(), "test_a");
        assert_eq!(obj.read_file(QString::from(&rfb)).to_string(), "test_b");
    }

    #[test]
    fn test_basename() {
        panic!("Not implemented");
    }

    #[test]
    fn test_is_absolute() {
        panic!("Not implemented");
    }

    #[test]
    fn test_realpath() {
        panic!("Not implemented");
    }

    #[test]
    fn test_exists() {
        panic!("Not implemented");
    }

    #[test]
    fn test_glob() {
        panic!("Not implemented");
    }
}