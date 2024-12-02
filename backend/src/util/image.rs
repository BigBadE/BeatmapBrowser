use std::collections::HashMap;
use std::io::Cursor;
use anyhow::Error;
use image::codecs::png::PngEncoder;
use image::{ImageEncoder, ImageFormat, ImageReader, PixelWithColorType, Rgb, RgbImage};
use crate::api::APIError;
use crate::parsing::BackgroundData;
use crate::util::data;
use crate::util::database::MapID;

const SUPPORTED_FORMATS: [ImageFormat; 3] = [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Bmp];

pub async fn save_image(
    image: &Option<Vec<u8>>,
    bg_data: &Option<BackgroundData>,
    uuid: &MapID,
) -> Result<(), APIError> {
    let Some(ref image) = image else {
        return Ok(());
    };
    if image.is_empty() {
        return Ok(());
    }
    let reader = ImageReader::new(Cursor::new(image)).with_guessed_format()?;
    if !reader
        .format()
        .is_some_and(|format| SUPPORTED_FORMATS.contains(&format))
    {
        return Err(APIError::ZipError(Error::msg(format!(
            "Unknown or unsupported background image format, please use png, bmp or jpeg! {:?}",
            reader.format()
        ))));
    }

    let image = reader
        .decode()
        .map_err(|err| APIError::ZipError(Error::from(err)))?;
    let size = (image.width(), image.height());
    let mut output = Vec::new();
    let image = replace_image_channels(image.to_rgb8(), size, bg_data);
    PngEncoder::new(&mut output).write_image(image.as_ref(), size.0, size.1, <Rgb<u8> as PixelWithColorType>::COLOR_TYPE)
        .map_err(|err| APIError::ZipError(err.into()))?;
    data().await.amazon.upload_object(output, format!("{uuid}.png").as_str()).await
        .map_err(APIError::database_error)?;
    Ok(())
}

fn replace_image_channels(
    mut img_buffer: RgbImage,
    size: (u32, u32),
    bg_data: &Option<BackgroundData>,
) -> RgbImage {
    let Some(bg_data) = bg_data else {
        return img_buffer;
    };
    let mut channels: HashMap<[u8; 3], [u8; 3]> = HashMap::new();
    if let Some(channel) = &bg_data.red_channel {
        channels.insert([255, 0, 0], channel.into());
    }
    if let Some(channel) = &bg_data.green_channel {
        channels.insert([0, 255, 0], channel.into());
    }
    if let Some(channel) = &bg_data.blue_channel {
        channels.insert([0, 0, 255], channel.into());
    }
    if let Some(channel) = &bg_data.magenta_channel {
        channels.insert([255, 0, 255], channel.into());
    }
    if let Some(channel) = &bg_data.cyan_channel {
        channels.insert([0, 255, 255], channel.into());
    }
    if let Some(channel) = &bg_data.yellow_channel {
        channels.insert([255, 255, 0], channel.into());
    }
    channels.insert([255, 255, 255], [255, 255, 255]);
    let mut i = 0;
    for pixel in img_buffer.pixels_mut() {
        if let Some(replacement) = channels.get(&pixel.0) {
            pixel.0 = *replacement;
        } else {
            pixel.0 = if ((i % size.0) % 2 == 0) && (i / size.0) % 2 == 0 {
                [0, 0, 0]
            } else {
                [255, 0, 255]
            }
        }
        i += 1;
    }
    img_buffer
}