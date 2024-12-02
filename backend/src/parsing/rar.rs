use crate::api::upload::MAX_SIZE;
use crate::parsing::ArchiveParser;
use anyhow::{Context, Error};
use std::env::temp_dir;
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::{fs, mem};
use unrar::{Archive, CursorBeforeFile, CursorBeforeHeader, OpenArchive, Process};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub struct RarArchiveReader<'a> {
    temp_file: PathBuf,
    file: &'a mut Vec<u8>,
}

impl<'a> RarArchiveReader<'a> {
    pub fn new(file: &'a mut Vec<u8>) -> Result<Self, Error> {
        let temp_file = temp_dir().join("beatblockbrowser_temp_download.rar");
        let mut temp = Vec::new();
        mem::swap(file, &mut temp);
        fs::write(&temp_file, temp)?;
        Ok(Self { temp_file, file })
    }
}

impl ArchiveParser for RarArchiveReader<'_> {
    fn fetch_file(&self, target_file_name: &str) -> Result<Vec<u8>, Error> {
        let file_name = target_file_name.to_ascii_lowercase();

        Ok(
            Iter::new(|name| !name.ends_with(&file_name), &self.temp_file)?
                .collect::<Result<Vec<_>, Error>>()?
                .into_iter()
                .filter_map(|value| value)
                .min_by_key(|(name, _)| name.matches('/').count())
                .context(format!("Failed to find the file {target_file_name}"))?
                .1,
        )
    }

    fn overwrite_file(&mut self) -> Result<(), Error> {
        let output = Vec::new();
        let mut zip = ZipWriter::new(Cursor::new(output));
        let mut archive = Archive::new(&self.temp_file).open_for_processing()?;
        while let Some(header) = archive.read_header()? {
            let file_name = match header.entry().filename.to_str() {
                Some(name) => name,
                None => return Err(Error::msg("Invalid file name in rar")),
            }
            .to_string();
            let (file_data, next) = header.read()?;
            if !file_data.is_empty() {
                zip.start_file(&file_name, SimpleFileOptions::default())?;
                zip.write(&file_data)?;
            }
            archive = next;
        }

        *self.file = zip.finish()?.into_inner();
        if self.file.len() > MAX_SIZE as usize {
            return Err(Error::msg("File size is too large!"));
        }
        Ok(())
    }
}

impl Drop for RarArchiveReader<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.temp_file);
    }
}

struct Iter<F: Fn(&String) -> bool> {
    archive: Option<OpenArchive<Process, CursorBeforeHeader>>,
    skip: F,
}

impl<F: Fn(&String) -> bool> Iter<F> {
    pub fn new(skip: F, archive: &PathBuf) -> Result<Self, Error> {
        Ok(Iter {
            archive: Some(Archive::new(archive).open_for_processing()?),
            skip,
        })
    }

    fn check_entry(
        &mut self,
        header: OpenArchive<Process, CursorBeforeFile>,
    ) -> Result<Option<(String, Vec<u8>)>, Error> {
        let name = header.entry().filename.clone();
        let name = name.to_str().unwrap_or("invalid_file_name");
        if (self.skip)(&name.to_ascii_lowercase()) {
            self.archive = Some(header.skip()?);
            Ok(None)
        } else {
            let (read, archive) = header.read()?;
            self.archive = Some(archive);
            Ok(Some((name.to_string(), read)))
        }
    }
}

impl<F: Fn(&String) -> bool> Iterator for Iter<F> {
    type Item = Result<Option<(String, Vec<u8>)>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let Some(header) = (match self.archive.take().unwrap().read_header() {
            Ok(header) => header,
            Err(error) => return Some(Err(error.into())),
        }) else {
            return None;
        };
        Some(self.check_entry(header))
    }
}
