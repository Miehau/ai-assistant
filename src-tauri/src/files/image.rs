// src-tauri/src/files/image.rs
use image::{DynamicImage, GenericImageView, ImageFormat, ImageError};
use std::io::Cursor;

pub struct ImageProcessor;

impl ImageProcessor {
    // Generate a thumbnail for an image
    pub fn generate_thumbnail(
        data: &[u8], 
        max_width: u32, 
        max_height: u32
    ) -> Result<Vec<u8>, ImageError> {
        let img = image::load_from_memory(data)?;
        let thumbnail = Self::resize_image(&img, max_width, max_height);
        
        let mut buffer = Cursor::new(Vec::new());
        thumbnail.write_to(&mut buffer, ImageFormat::Jpeg)?;
        
        Ok(buffer.into_inner())
    }
    
    // Optimize an image by resizing and compressing it
    pub fn optimize_image(
        data: &[u8], 
        max_width: u32, 
        max_height: u32, 
        quality: u8
    ) -> Result<Vec<u8>, ImageError> {
        let img = image::load_from_memory(data)?;
        let format = image::guess_format(data)?;
        
        // Only resize if the image is larger than the max dimensions
        let (width, height) = img.dimensions();
        let resized = if width > max_width || height > max_height {
            Self::resize_image(&img, max_width, max_height)
        } else {
            img
        };
        
        let mut buffer = Cursor::new(Vec::new());
        
        // Use the original format for the optimized image
        match format {
            ImageFormat::Jpeg => {
                let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, quality);
                encoder.encode_image(&resized)?;
            },
            ImageFormat::Png => {
                resized.write_to(&mut buffer, ImageFormat::Png)?;
            },
            _ => {
                // Default to JPEG for other formats
                let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, quality);
                encoder.encode_image(&resized)?;
            }
        }
        
        Ok(buffer.into_inner())
    }
    
    // Resize an image while maintaining aspect ratio
    fn resize_image(img: &DynamicImage, max_width: u32, max_height: u32) -> DynamicImage {
        let (width, height) = img.dimensions();
        
        if width <= max_width && height <= max_height {
            return img.clone();
        }
        
        let ratio = width as f32 / height as f32;
        let (new_width, new_height) = if width > height {
            let new_width = max_width;
            let new_height = (new_width as f32 / ratio) as u32;
            (new_width, new_height)
        } else {
            let new_height = max_height;
            let new_width = (new_height as f32 * ratio) as u32;
            (new_width, new_height)
        };
        
        img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
    }
    
    // Extract metadata from an image
    pub fn extract_metadata(data: &[u8]) -> Result<serde_json::Value, ImageError> {
        let img = image::load_from_memory(data)?;
        let (width, height) = img.dimensions();
        let format = image::guess_format(data)?;
        
        // Convert ImageFormat to a string manually since it doesn't implement Display
        let format_str = match format {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpeg",
            ImageFormat::Gif => "gif",
            ImageFormat::WebP => "webp",
            ImageFormat::Pnm => "pnm",
            ImageFormat::Tiff => "tiff",
            ImageFormat::Tga => "tga",
            ImageFormat::Dds => "dds",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Ico => "ico",
            ImageFormat::Hdr => "hdr",
            ImageFormat::OpenExr => "openexr",
            ImageFormat::Farbfeld => "farbfeld",
            ImageFormat::Avif => "avif",
            _ => "unknown",
        };
        
        let metadata = serde_json::json!({
            "width": width,
            "height": height,
            "format": format_str,
            "aspect_ratio": width as f32 / height as f32,
            "size_bytes": data.len(),
        });
        
        Ok(metadata)
    }
}
