use std::sync::Arc;

use custom_error::custom_error;
use tile_info::ImageInfo;

use crate::dezoomer::*;

mod tile_info;

#[derive(Default)]
pub struct IIIF;

custom_error! {pub IIIFError
    JsonError{source: serde_json::Error} = "Invalid IIIF info.json file: {source}"
}

impl From<IIIFError> for DezoomerError {
    fn from(err: IIIFError) -> Self {
        DezoomerError::Other { source: err.into() }
    }
}

impl Dezoomer for IIIF {
    fn name(&self) -> &'static str {
        "iiif"
    }

    fn zoom_levels(&mut self, data: &DezoomerInput) -> Result<ZoomLevels, DezoomerError> {
        self.assert(data.uri.ends_with("/info.json"))?;
        let with_contents = data.with_contents()?;
        let contents = with_contents.contents;
        let uri = with_contents.uri;
        Ok(zoom_levels(uri, contents)?)
    }
}

fn zoom_levels(url: &str, raw_info: &[u8]) -> Result<ZoomLevels, IIIFError> {
    let image_info: ImageInfo = serde_json::from_slice(raw_info)?;
    let img = Arc::new(image_info);
    let default_tiles = vec![Default::default()];
    let tiles = img.tiles.as_ref().unwrap_or(&default_tiles);
    let base_url = &Arc::new(url.replace("/info.json", ""));
    let levels = tiles
        .iter()
        .flat_map(|tile_info| {
            let tile_size = Vec2d {
                x: tile_info.width,
                y: tile_info.height.unwrap_or(tile_info.width),
            };
            let page_info = &img; // Required to allow the move
            tile_info
                .scale_factors
                .iter()
                .map(move |&scale_factor| IIIFZoomLevel {
                    scale_factor,
                    tile_size,
                    page_info: Arc::clone(page_info),
                    base_url: Arc::clone(base_url),
                })
        })
        .into_zoom_levels();
    Ok(levels)
}

struct IIIFZoomLevel {
    scale_factor: u32,
    tile_size: Vec2d,
    page_info: Arc<ImageInfo>,
    base_url: Arc<String>,
}

impl TilesRect for IIIFZoomLevel {
    fn size(&self) -> Vec2d {
        self.page_info.size() / self.scale_factor
    }

    fn tile_size(&self) -> Vec2d {
        self.tile_size
    }

    fn tile_url(&self, col_and_row_pos: Vec2d) -> String {
        let scaled_tile_size = self.tile_size * self.scale_factor;
        let xy_pos = col_and_row_pos * scaled_tile_size;
        let scaled_tile_size = max_size_in_rect(xy_pos, scaled_tile_size, self.page_info.size());
        let tile_size = scaled_tile_size / self.scale_factor;
        format!(
            "{base}/{x},{y},{img_w},{img_h}/{tile_w},{tile_h}/{rotation}/{quality}.{format}",
            base = self.page_info.id.as_ref().unwrap_or(&self.base_url),
            x = xy_pos.x,
            y = xy_pos.y,
            img_w = scaled_tile_size.x,
            img_h = scaled_tile_size.y,
            tile_w = tile_size.x,
            tile_h = tile_size.y,
            rotation = 0,
            quality = "default",
            format = "jpg"
        )
    }
}

impl std::fmt::Debug for IIIFZoomLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let name = self
            .base_url
            .split('/')
            .last()
            .and_then(|s: &str| {
                let s = s.trim();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            })
            .unwrap_or("IIIF Image");
        write!(f, "{}", name)
    }
}

#[test]
fn test_tiles() {
    let data = br#"{
      "@context" : "http://iiif.io/api/image/2/context.json",
      "@id" : "http://www.asmilano.it/fast/iipsrv.fcgi?IIIF=/opt/divenire/files/./tifs/05/36/536765.tif",
      "protocol" : "http://iiif.io/api/image",
      "width" : 15001,
      "height" : 48002,
      "tiles" : [
         { "width" : 512, "height" : 512, "scaleFactors" : [ 1, 2, 4, 8, 16, 32, 64, 128 ] }
      ],
      "profile" : [
         "http://iiif.io/api/image/2/level1.json",
         { "formats" : [ "jpg" ],
           "qualities" : [ "native","color","gray" ],
           "supports" : ["regionByPct","sizeByForcedWh","sizeByWh","sizeAboveFull","rotationBy90s","mirroring","gray"] }
      ]
    }"#;
    let mut levels = zoom_levels("test.com", data).unwrap();
    let tiles: Vec<String> = levels[6]
        .next_tiles(None)
        .into_iter()
        .map(|t| t.url)
        .collect();
    assert_eq!(tiles, vec![
        "http://www.asmilano.it/fast/iipsrv.fcgi?IIIF=/opt/divenire/files/./tifs/05/36/536765.tif/0,0,15001,32768/234,512/0/default.jpg",
        "http://www.asmilano.it/fast/iipsrv.fcgi?IIIF=/opt/divenire/files/./tifs/05/36/536765.tif/0,32768,15001,15234/234,238/0/default.jpg",
    ])
}

#[test]
fn test_missing_id() {
    let data = br#"{
      "width" : 600,
      "height" : 350
    }"#;
    let mut levels = zoom_levels("http://test.com/info.json", data).unwrap();
    let tiles: Vec<String> = levels[0]
        .next_tiles(None)
        .into_iter()
        .map(|t| t.url)
        .collect();
    assert_eq!(
        tiles,
        vec![
            "http://test.com/0,0,512,350/512,350/0/default.jpg",
            "http://test.com/512,0,88,350/88,350/0/default.jpg"
        ]
    )
}