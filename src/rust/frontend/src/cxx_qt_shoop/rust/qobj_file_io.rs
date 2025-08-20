use backend_bindings::MultichannelAudio;
use common::logging::macros::*;
use cxx_qt_lib::QList;
use cxx_qt_lib_shoop::connection_types;
use cxx_qt_lib_shoop::invokable::invoke;
use cxx_qt_lib_shoop::qvariant_helpers::{qvariant_to_qlist_f32, qvariant_to_qvariantlist};
use sndfile::{SndFileIO, SndFileNDArrayIO};
shoop_log_unit!("Frontend.FileIO");

use crate::cxx_qt_shoop::qobj_file_io_bridge::ffi::*;
pub use crate::cxx_qt_shoop::qobj_file_io_bridge::FileIORust;

use dunce;
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

pub fn register_qml_singleton(module_name: &str, type_name: &str) {
    let mut mdl = String::from(module_name);
    let mut tp = String::from(type_name);
    unsafe {
        register_qml_singleton_file_io(std::ptr::null_mut(), &mut mdl, 1, 0, &mut tp);
    }
}

fn get_formats_for(filename: &Path) -> Option<(sndfile::MajorFormat, sndfile::SubtypeFormat)> {
    todo!();
}

fn save_data_to_soundfile_impl<'a>(
    filename: &Path,
    samplerate: usize,
    data: impl Iterator<Item = &'a f32>, // inner = frame, outer = channel
    n_frames: usize,
    n_channels: usize,
) -> Result<(), anyhow::Error> {
    let (major_format, subtype_format) = get_formats_for(filename).ok_or(anyhow::anyhow!(
        "No format available for filename: {filename:?}"
    ))?;

    // Reshape
    let mut reshaped: Vec<f32> = Vec::default();
    {
        reshaped.resize(n_frames * n_channels, 0.0);
        data.enumerate().for_each(|(idx, val)| {
            let chan = idx / n_frames;
            let frame = idx % n_frames;
            reshaped[frame * n_channels + chan] = *val;
        });
    }
    sndfile::OpenOptions::WriteOnly(sndfile::WriteOptions::new(
        major_format,
        subtype_format,
        sndfile::Endian::File,
        samplerate,
        n_channels,
    ))
    .from_path(filename)
    .map_err(|e| anyhow::anyhow!("could not open sound file: {e:?}"))?
    .write_from_iter(reshaped.iter().map(|v| *v))
    .map_err(|e| anyhow::anyhow!("could not write sound file: {e:?}"))?;
    Ok(())
}

fn load_soundfile_to_channels_impl(
    filename: &Path,
    target_sample_rate: usize,
    maybe_target_data_length: Option<usize>,
    channels_to_loop_channels: &HashMap<usize, HashSet<*mut QObject>>,
    maybe_set_n_preplay_samples: Option<usize>,
    maybe_set_start_offset: Option<isize>,
    maybe_update_loop_to_data_length: Option<*mut QObject>,
) {
    if let Err(e) = || -> Result<(), anyhow::Error> {
        let combined_data: Vec<f32>;
        let n_channels: usize;
        let file_sample_rate: usize;
        {
            let mut sound_file = sndfile::OpenOptions::ReadOnly(sndfile::ReadOptions::Auto)
                .from_path(filename)
                .map_err(|e| anyhow::anyhow!("could not open sound file for reading: {e:?}"))?;
            n_channels = sound_file.get_channels();
            file_sample_rate = sound_file.get_samplerate();
            combined_data = sound_file
                .read_all_to_vec()
                .map_err(|e| anyhow::anyhow!("could not read data from sound file: {e:?}"))?;
        }
        if n_channels == 0 {
            return Err(anyhow::anyhow!("got 0-channel audio"));
        }
        let n_frames = combined_data.len() / n_channels;

        let mut multi_audio: MultichannelAudio =
            MultichannelAudio::new(n_channels as u32, n_frames as u32)?;
        combined_data
            .iter()
            .enumerate()
            .try_for_each(|(idx, v)| -> Result<(), anyhow::Error> {
                let chan_idx = idx % n_channels;
                let frame_idx = idx / n_channels;
                multi_audio.set(frame_idx as u32, chan_idx as u32, *v)?;
                Ok(())
            })?;

        let mut target_frames: usize = n_frames;
        if target_sample_rate != file_sample_rate {
            let filename = filename.file_name().unwrap().to_str().unwrap();
            debug!("Resample {filename} from {file_sample_rate}Hz to {target_sample_rate}Hz, explicit target length {maybe_target_data_length:?}");
            target_frames = maybe_target_data_length
                .unwrap_or((n_frames * target_sample_rate) / file_sample_rate);
            multi_audio = multi_audio.resample(target_frames as u32)?
        }

        let mut channel_datas: Vec<QList_f32> = Vec::default();
        for _ in 0..n_channels {
            channel_datas.push(QList::default());
            channel_datas
                .last_mut()
                .unwrap()
                .reserve(target_frames as isize);
        }

        for frame in 0..target_frames {
            for chan in 0..n_channels {
                channel_datas
                    .get_mut(chan)
                    .unwrap()
                    .append(multi_audio.at(frame as u32, chan as u32)?);
            }
        }

        channel_datas.iter().enumerate().try_for_each(
            |(idx, qlist)| -> Result<(), anyhow::Error> {
                if let Some(target_channels) = channels_to_loop_channels.get(&idx) {
                    for target_channel in target_channels {
                        unsafe {
                            invoke::<_, (), _>(
                                &mut **target_channel,
                                "load_audio_data(QList<float>)",
                                connection_types::DIRECT_CONNECTION,
                                &(*qlist),
                            )?;
                            if let Some(start_offset) = maybe_set_start_offset {
                                invoke::<_, (), _>(
                                    &mut **target_channel,
                                    "set_start_offset(::std::int32_t)",
                                    connection_types::DIRECT_CONNECTION,
                                    &(start_offset as i32),
                                )?;
                            }
                            if let Some(n_preplay) = maybe_set_n_preplay_samples {
                                invoke::<_, (), _>(
                                    &mut **target_channel,
                                    "set_n_preplay_samples(::std::int32_t)",
                                    connection_types::DIRECT_CONNECTION,
                                    &(n_preplay as i32),
                                )?;
                            }
                        }
                        debug!("Loaded data for single channel");
                    }
                }
                Ok(())
            },
        )?;

        if let Some(loop_obj) = maybe_update_loop_to_data_length {
            unsafe {
                invoke::<_, (), _>(
                    &mut *loop_obj,
                    "queue_set_length(::std::int32_t)",
                    connection_types::DIRECT_CONNECTION,
                    &(target_frames as i32),
                )?;
            }
        }

        info!("Loaded {n_channels}-channel audio from {filename:?} ({target_frames} samples)");

        Ok(())
    }() {
        error!("Could not load sound file into channels: {e}");
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

    pub fn save_data_to_soundfile(
        self: &FileIO,
        filename: QString,
        samplerate: i32,
        data: QVariant,
    ) {
        if let Err(e) = || -> Result<(), anyhow::Error> {
            let filename = PathBuf::from(filename.to_string());
            if let Ok(mono) = qvariant_to_qlist_f32(&data) {
                save_data_to_soundfile_impl(
                    &filename,
                    samplerate as usize,
                    mono.iter(),
                    mono.len() as usize,
                    1,
                )?;
            } else if let Ok(multi) = qvariant_to_qvariantlist(&data) {
                let mut n_frames: Option<usize> = None;
                let n_channels: usize = multi.len() as usize;
                let mut qlists : Vec<QList_f32> = Vec::default();

                for variant in multi.iter() {
                    let qlist = qvariant_to_qlist_f32(&variant)
                        .map_err(|e| anyhow::anyhow!("Could not view data as f32 list: {e}"))?;
                    if n_frames.is_none() {
                        n_frames = Some(qlist.len() as usize);
                    } else if qlist.len() as usize != n_frames.unwrap() {
                        return Err(anyhow::anyhow!("Inconsistent data size across channels"));
                    }
                    qlists.push(qlist);
                }

                let total_data_iter =
                    qlists.iter().flat_map(|list| list.iter());

                save_data_to_soundfile_impl(
                    &filename,
                    samplerate as usize,
                    total_data_iter,
                    n_frames.unwrap(),
                    n_channels,
                )?;
            } else {
                return Err(anyhow::anyhow!(
                    "Unsupported QVariant type for saving audio file."
                ));
            }
            Ok(())
        }() {
            error!("Failed to save audio data: {e}");
        }
    }

    pub fn save_channel_to_midi_async(
        self: &FileIO,
        filename: QString,
        samplerate: i32,
        channel: *mut QObject,
    ) -> *mut QObject {
        todo!();
    }

    pub fn save_channel_to_midi(
        self: &FileIO,
        filename: QString,
        samplerate: i32,
        channel: *mut QObject,
    ) {
        todo!();
    }

    pub fn load_midi_to_channels_async(
        self: &FileIO,
        samplerate: i32,
        channels: QList_QVariant,
        maybe_set_n_preplay_samples: QVariant,
        maybe_set_start_offset: QVariant,
        maybe_update_loop_to_data_length: QVariant,
    ) -> *mut QObject {
        todo!();
    }

    pub fn load_midi_to_channels(
        self: &FileIO,
        samplerate: i32,
        channels: QList_QVariant,
        maybe_set_n_preplay_samples: QVariant,
        maybe_set_start_offset: QVariant,
        maybe_update_loop_to_data_length: QVariant,
    ) {
        todo!();
    }

    pub fn save_channels_to_soundfile_async(
        self: &FileIO,
        filename: QString,
        samplerate: i32,
        channels: QList_QVariant,
    ) -> *mut QObject {
        todo!();
    }

    pub fn save_channels_to_soundfile(
        self: &FileIO,
        filename: QString,
        samplerate: i32,
        channels: QList_QVariant,
    ) {
        todo!();
    }

    pub fn load_soundfile_to_channels_async(
        self: &FileIO,
        filename: QString,
        target_samplerate: i32,
        maybe_target_data_length: QVariant,
        channels_to_loop_channels: QList_QVariant,
        maybe_set_n_preplay_samples: QVariant,
        maybe_set_start_offset: QVariant,
        maybe_update_loop_to_data_length: QVariant,
    ) -> *mut QObject {
        todo!();
    }

    pub fn load_soundfile_to_channels(
        self: &FileIO,
        filename: QString,
        target_samplerate: i32,
        maybe_target_data_length: QVariant,
        channels_to_loop_channels: QList_QVariant,
        maybe_set_n_preplay_samples: QVariant,
        maybe_set_start_offset: QVariant,
        maybe_update_loop_to_data_length: QVariant,
    ) {
        todo!();
    }

    pub fn get_soundfile_info(self: &FileIO, filename: QString) -> QMap_QString_QVariant {
        todo!();
    }

    pub fn get_soundfile_formats(self: &FileIO) -> QMap_QString_QVariant {
        todo!();
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
