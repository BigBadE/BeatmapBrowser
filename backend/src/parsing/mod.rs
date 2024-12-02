use crate::api::upload::MAX_SIZE;
use crate::api::APIError;
use crate::parsing::rar::RarArchiveReader;
use crate::parsing::zip::ZipArchiveReader;
use ::zip::write::SimpleFileOptions;
use ::zip::{ZipArchive, ZipWriter};
use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Write};
use std::path::{Component, PathBuf};

pub mod rar;
pub mod zip;

pub struct FileData {
    pub level_data: LevelMetadata,
    pub image: Option<Vec<u8>>,
}

#[derive(Deserialize)]
pub struct LevelData {
    pub metadata: LevelMetadata,
}

#[derive(Deserialize)]
pub struct LevelMetadata {
    pub artist: String,
    pub charter: String,
    pub difficulty: Option<f64>,
    pub description: String,
    #[serde(rename = "songName")]
    pub song_name: String,
    #[serde(rename = "artistList")]
    #[serde(default)]
    pub artist_list: String,
    #[serde(rename = "bgData")]
    #[serde(default)]
    pub bg_data: Option<BackgroundData>,
    #[serde(default)]
    pub variants: Vec<LevelVariant>,
}

#[derive(Deserialize)]
pub struct BackgroundData {
    image: String,
    #[serde(rename = "cyanChannel")]
    pub cyan_channel: Option<ColorChannel>,
    #[serde(rename = "magentaChannel")]
    pub magenta_channel: Option<ColorChannel>,
    #[serde(rename = "yellowChannel")]
    pub yellow_channel: Option<ColorChannel>,
    #[serde(rename = "redChannel")]
    pub red_channel: Option<ColorChannel>,
    #[serde(rename = "greenChannel")]
    pub green_channel: Option<ColorChannel>,
    #[serde(rename = "blueChannel")]
    pub blue_channel: Option<ColorChannel>,
}

#[derive(Deserialize)]
pub struct ColorChannel {
    #[serde(rename = "r")]
    red: u8,
    #[serde(rename = "g")]
    green: u8,
    #[serde(rename = "b")]
    blue: u8
}

impl Into<[u8; 3]> for &ColorChannel {
    fn into(self) -> [u8; 3] {
        [self.red, self.green, self.blue]
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct LevelVariant {
    display: String,
    difficulty: f64,
}

impl Into<LevelVariant> for f64 {
    fn into(self) -> LevelVariant {
        LevelVariant {
            display: get_difficulty(self),
            difficulty: self,
        }
    }
}

fn get_difficulty(difficulty: f64) -> String {
    match difficulty {
        ..=0.0 => "Special",
        ..=5.0 => "Easy",
        ..=10.0 => "Hard",
        ..=15.0 => "Challenge",
        _ => "Apocrypha",
    }
    .to_string()
}

pub fn check_path(path: &PathBuf) -> Result<(), Error> {
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        Err(Error::msg("File path contains directory traversal"))
    } else {
        Ok(())
    }
}

pub fn get_parser<'a>(beatmap: &'a mut Vec<u8>) -> Result<Box<dyn ArchiveParser + 'a>, APIError> {
    Ok(if beatmap.starts_with("PK".as_bytes()) {
        Box::new(ZipArchiveReader::new(beatmap).map_err(|err| APIError::ZipError(err))?)
    } else if beatmap.starts_with("Rar".as_bytes()) {
        Box::new(RarArchiveReader::new(beatmap).map_err(|err| APIError::ZipError(err))?)
    } else {
        if beatmap.len() > 3 {
            println!("Bad archive {:?}", &beatmap[0..3]);
        }
        return Err(APIError::ArchiveTypeError());
    })
}

pub fn parse_archive(archive_parser: &mut dyn ArchiveParser) -> Result<FileData, Error> {
    let data = archive_parser
        .fetch_file("level.json")
        .and_then(|data| serde_json::from_slice::<LevelData>(&data).map_err(Error::new))
        .or_else(|_| {
            archive_parser
                .fetch_file("manifest.json")
                .and_then(|data| serde_json::from_slice(&data).map_err(Error::new))
        })?;

    let metadata = data.metadata;
    let mut image = None;
    if let Some(bg_data) = metadata.bg_data.as_ref() {
        if !bg_data.image.is_empty() {
            image = archive_parser.fetch_file(&bg_data.image).map_or(None, Some);
        }
    }

    archive_parser.overwrite_file()?;

    Ok(FileData {
        level_data: metadata,
        image,
    })
}

pub fn check_archive(file: &mut Vec<u8>) -> Result<(), Error> {
    let output = Vec::new();
    let mut zip = ZipWriter::new(Cursor::new(output));
    let mut cursor: Cursor<&Vec<u8>> = Cursor::new(file);
    let mut archive = ZipArchive::new(&mut cursor)?;
    let mut size = 0;
    let files: Vec<String> = archive
        .file_names()
        .map(|string| string.to_string())
        .collect();
    for file_name in files {
        if !is_legal_name(&file_name)? {
            continue;
        }
        let mut file = archive.by_name(&file_name)?;
        let file_size = file.size();
        // Prevent overflows
        if (size + file_size).max(file_size) > (MAX_SIZE * 2) as u64 {
            return Err(Error::msg("Uncompressed file size is too large!"));
        }
        size += file_size;
        zip.start_file(file_name, SimpleFileOptions::default())?;
        let mut zip_file = Vec::new();
        file.read_to_end(&mut zip_file)?;
        zip.write_all(&zip_file)?;
    }
    *file = zip.finish()?.into_inner();
    Ok(())
}

// Allows misspelling, just here to block exes and other malicious files
pub const EXTENSIONS: [&'static str; 12] = [
    "png", "jpg", "jpeg", "webp", "mp3", "bmp", "ogg", "oog", "wav", "json", "md", "txt",
];

fn is_legal_name(name: &str) -> Result<bool, Error> {
    check_path(&PathBuf::from(name))?;
    Ok(name.ends_with('/')
        || name.ends_with('\\')
        || EXTENSIONS.contains(&name.split('.').last().unwrap()))
}

pub trait ArchiveParser {
    fn fetch_file(&self, target_file_name: &str) -> Result<Vec<u8>, Error>;

    fn overwrite_file(&mut self) -> Result<(), Error>;
}
