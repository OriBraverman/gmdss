//! Panel artwork from SVG files (`assets/panels`): rendered to a texture at the size shown, and
//! looked up by element id for the controls and windows the code draws on.

use eframe::egui::{self, Rect, TextureHandle, Vec2};
use resvg::{tiny_skia, usvg};
use std::sync::{Arc, OnceLock};

/// The bundled fonts; SVG text uses `DejaVu Sans`.
fn fonts() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_font_data(include_bytes!("../assets/fonts/DejaVuSans.ttf").to_vec());
        db.load_font_data(include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf").to_vec());
        Arc::new(db)
    })
    .clone()
}

pub struct Art {
    name: &'static str,
    tree: usvg::Tree,
    /// Texture and the pixel width it was rendered at.
    texture: Option<(u32, TextureHandle)>,
}

impl Art {
    pub fn new(name: &'static str, svg: &str) -> Art {
        let options = usvg::Options { fontdb: fonts(), ..Default::default() };
        let tree = usvg::Tree::from_str(svg, &options).unwrap_or_else(|e| panic!("{name}: {e}"));
        Art { name, tree, texture: None }
    }

    /// Size in SVG units.
    pub fn size(&self) -> Vec2 {
        Vec2::new(self.tree.size().width(), self.tree.size().height())
    }

    /// Bounding box of element `id` in SVG units.
    pub fn rect(&self, id: &str) -> Rect {
        let node = self.tree.node_by_id(id).unwrap_or_else(|| panic!("{}: no element `{id}`", self.name));
        let b = node.abs_bounding_box();
        Rect::from_min_size(egui::pos2(b.x(), b.y()), Vec2::new(b.width(), b.height()))
    }

    /// The artwork as a texture `px` pixels wide (rendered again only when the width changes).
    pub fn texture(&mut self, ctx: &egui::Context, px: u32) -> &TextureHandle {
        if self.texture.as_ref().is_none_or(|(w, _)| *w != px) {
            let scale = px as f32 / self.size().x;
            let h = (self.size().y * scale).ceil() as u32;
            let mut pixmap = tiny_skia::Pixmap::new(px.max(1), h.max(1)).expect("pixmap");
            resvg::render(&self.tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
            let image = egui::ColorImage::from_rgba_premultiplied([pixmap.width() as usize, pixmap.height() as usize], pixmap.data());
            self.texture = Some((px, ctx.load_texture(self.name, image, egui::TextureOptions::LINEAR)));
        }
        &self.texture.as_ref().unwrap().1
    }
}
