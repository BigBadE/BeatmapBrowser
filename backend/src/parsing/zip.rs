use crate::parsing::ArchiveParser;
use anyhow::{Context, Error};
use std::io::{Cursor, Read};
use zip::ZipArchive;

pub struct ZipArchiveReader<'a> {
    file: &'a mut Vec<u8>,
}

impl<'a> ZipArchiveReader<'a> {
    pub fn new(file: &'a mut Vec<u8>) -> Result<Self, Error> {
        Ok(Self { file })
    }
}

impl ArchiveParser for ZipArchiveReader<'_> {
    fn fetch_file(&self, target_file_name: &str) -> Result<Vec<u8>, Error> {
        let mut cursor: Cursor<&Vec<u8>> = Cursor::new(self.file);
        let mut archive = ZipArchive::new(&mut cursor)?;
        let target_file_name = target_file_name.to_ascii_lowercase();
        let mut output = Vec::new();
        archive
            .by_name(
                &archive
                    .file_names()
                    .filter(|name| name.to_ascii_lowercase().ends_with(&target_file_name))
                    .min_by_key(|name| name.matches('/').count())
                    .context(format!("Failed to find the file {target_file_name}"))?
                    .to_string(),
            )?
            .read_to_end(&mut output)?;
        Ok(output)
    }

    fn overwrite_file(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
