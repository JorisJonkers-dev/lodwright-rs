// SPDX-License-Identifier: GPL-3.0-only
//! Packing the textures of object LOD into atlases: one when they fit, more pages at native size when they do not.
//!
//! The game's own atlas holds every texture its models use at their native size (the measured textures add up to
//! 97 percent of the atlas area), so the packer keeps native sizes and picks the smallest atlas that fits, halving
//! only a texture that cannot fit at all. Shelf packing of power-of-two sizes sorted by height and width wastes
//! almost nothing.

use std::collections::BTreeMap;

/// A rectangle of the atlas in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

/// The placement of every texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atlas {
    /// Atlas width in pixels.
    pub width: u32,
    /// Atlas height in pixels.
    pub height: u32,
    /// Rectangle per texture key.
    pub rects: BTreeMap<String, Rect>,
}

const SIZES: [(u32, u32); 6] = [
    (2048, 1024),
    (2048, 2048),
    (4096, 2048),
    (4096, 4096),
    (8192, 4096),
    (8192, 8192),
];

fn shelf(items: &[(String, u32, u32)], width: u32, height: u32) -> Option<BTreeMap<String, Rect>> {
    let mut placed = BTreeMap::new();
    let (mut x, mut y, mut row) = (0_u32, 0_u32, 0_u32);
    for (key, w, h) in items {
        if x + w > width {
            y += row;
            x = 0;
            row = 0;
        }
        if row == 0 {
            row = *h;
        }
        if y + *h > height || *w > width {
            return None;
        }
        placed.insert(key.clone(), Rect { x, y, w: *w, h: *h });
        x += w;
    }
    Some(placed)
}

/// Pack textures given as (key, width, height). A texture wider than the widest atlas is halved until it fits.
/// The result is deterministic: items sort by height, width and key.
#[must_use]
pub fn pack(textures: &[(String, u32, u32)]) -> Atlas {
    let widest = SIZES[SIZES.len() - 1].0;
    let mut items: Vec<(String, u32, u32)> = textures
        .iter()
        .map(|(k, w, h)| {
            let (mut w, mut h) = ((*w).max(1), (*h).max(1));
            while w > widest || h > widest {
                w = (w / 2).max(1);
                h = (h / 2).max(1);
            }
            (k.clone(), w, h)
        })
        .collect();
    items.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)).then(a.0.cmp(&b.0)));
    for (width, height) in SIZES {
        if let Some(rects) = shelf(&items, width, height) {
            return Atlas {
                width,
                height,
                rects,
            };
        }
    }
    // nothing fits at native size: halve everything and try again
    let halved: Vec<(String, u32, u32)> = items
        .iter()
        .map(|(k, w, h)| (k.clone(), (w / 2).max(1), (h / 2).max(1)))
        .collect();
    pack(&halved)
}

/// The largest atlas [`pack_pages`] makes unless told otherwise, as the largest of the game's own sizes above.
pub const LARGEST_ATLAS: (u32, u32) = SIZES[SIZES.len() - 1];

/// Pack textures given as (key, width, height) into as many atlases (pages) as they need, each texture at its native
/// size: one page when everything fits an atlas no larger than `largest`, else full pages of that size and a last page
/// of the smallest size that holds the rest. Only a texture larger than `largest` is halved. The result is
/// deterministic: items sort by height, width and key, and fill the pages in that order.
#[must_use]
pub fn pack_pages(textures: &[(String, u32, u32)], largest: (u32, u32)) -> Vec<Atlas> {
    let (widest, tallest) = largest;
    let sizes: Vec<(u32, u32)> = SIZES
        .iter()
        .copied()
        .filter(|&(w, h)| w <= widest && h <= tallest)
        .chain(std::iter::once(largest))
        .collect();
    let mut items: Vec<(String, u32, u32)> = textures
        .iter()
        .map(|(k, w, h)| {
            let (mut w, mut h) = ((*w).max(1), (*h).max(1));
            while w > widest || h > tallest {
                w = (w / 2).max(1);
                h = (h / 2).max(1);
            }
            (k.clone(), w, h)
        })
        .collect();
    items.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)).then(a.0.cmp(&b.0)));
    let mut pages = Vec::new();
    let mut rest: &[(String, u32, u32)] = &items;
    loop {
        if let Some(atlas) = sizes.iter().find_map(|&(width, height)| {
            shelf(rest, width, height).map(|rects| Atlas {
                width,
                height,
                rects,
            })
        }) {
            pages.push(atlas);
            return pages;
        }
        // a full page of the largest size: the items in order until the first that does not fit
        let mut taken = 1;
        while taken < rest.len() && shelf(&rest[..=taken], widest, tallest).is_some() {
            taken += 1;
        }
        let rects = shelf(&rest[..taken], widest, tallest).unwrap_or_default();
        pages.push(Atlas {
            width: widest,
            height: tallest,
            rects,
        });
        rest = &rest[taken..];
    }
}

#[cfg(test)]
mod tests {
    use super::{LARGEST_ATLAS, Rect, pack, pack_pages};
    use lodwright_testkit::{covers, ids};

    #[test]
    fn textures_that_fit_one_atlas_make_one_page_as_pack_does() {
        covers(&[ids::objects::blocks::ATLAS_PAIR]);
        let few: Vec<(String, u32, u32)> = (0..40).map(|i| (format!("t{i}"), 512, 512)).collect();
        assert_eq!(pack_pages(&few, LARGEST_ATLAS), vec![pack(&few)]);
    }

    #[test]
    fn textures_that_do_not_fit_one_atlas_keep_their_size_on_more_pages() {
        covers(&[ids::objects::blocks::ATLAS_PAGES]);
        // 300 textures of 1024 by 1024 are 4.7 atlases of the largest size
        let many: Vec<(String, u32, u32)> =
            (0..300).map(|i| (format!("t{i:03}"), 1024, 1024)).collect();
        let pages = pack_pages(&many, LARGEST_ATLAS);
        assert_eq!(pages.len(), 5);
        let mut seen = 0;
        for page in &pages {
            assert!(page.width <= 8192 && page.height <= 8192);
            for r in page.rects.values() {
                assert_eq!((r.w, r.h), (1024, 1024));
                assert!(r.x + r.w <= page.width && r.y + r.h <= page.height);
            }
            seen += page.rects.len();
        }
        assert_eq!(seen, 300);
        assert_eq!((pages[0].width, pages[0].height), (8192, 8192));
        // the last page holds 44 textures and is the smallest size that does
        assert_eq!(pages[4].rects.len(), 44);
        assert_eq!((pages[4].width, pages[4].height), (8192, 8192));
        assert_eq!(pack_pages(&many, LARGEST_ATLAS), pages);
    }

    #[test]
    fn a_smaller_largest_atlas_makes_pages_of_that_size() {
        let five: Vec<(String, u32, u32)> = (0..5).map(|i| (format!("t{i}"), 1024, 1024)).collect();
        let pages = pack_pages(&five, (2048, 2048));
        assert_eq!(pages.len(), 2);
        assert_eq!(
            (pages[0].width, pages[0].height, pages[0].rects.len()),
            (2048, 2048, 4)
        );
        assert_eq!(
            (pages[1].width, pages[1].height, pages[1].rects.len()),
            (2048, 1024, 1)
        );
    }

    fn overlap(a: &Rect, b: &Rect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn rectangles_never_overlap_and_stay_inside() {
        covers(&[ids::objects::blocks::ATLAS_PAIR]);
        let textures: Vec<(String, u32, u32)> = (0..60)
            .map(|i| (format!("t{i:02}"), 64 << (i % 3), 64 << ((i / 3) % 2)))
            .collect();
        let atlas = pack(&textures);
        let rects: Vec<&Rect> = atlas.rects.values().collect();
        for (i, a) in rects.iter().enumerate() {
            assert!(a.x + a.w <= atlas.width && a.y + a.h <= atlas.height);
            for b in &rects[i + 1..] {
                assert!(!overlap(a, b), "{a:?} {b:?}");
            }
        }
        assert_eq!(atlas.rects.len(), 60);
    }

    #[test]
    fn the_smallest_size_that_fits_is_chosen_and_the_result_is_deterministic() {
        covers(&[ids::objects::blocks::ATLAS_PAIR]);
        let few = vec![("a".to_owned(), 128, 128), ("b".to_owned(), 64, 64)];
        assert_eq!((pack(&few).width, pack(&few).height), (2048, 1024));
        let many: Vec<(String, u32, u32)> = (0..40).map(|i| (format!("t{i}"), 512, 512)).collect();
        let atlas = pack(&many);
        assert!(atlas.width * atlas.height >= 40 * 512 * 512);
        assert_eq!(pack(&many), atlas);
        let mut reversed = many.clone();
        reversed.reverse();
        assert_eq!(pack(&reversed), atlas);
    }

    #[test]
    fn a_texture_larger_than_any_atlas_is_halved() {
        let atlas = pack(&[("big".to_owned(), 16384, 16384)]);
        let r = atlas.rects["big"];
        assert!(r.w <= 8192 && r.h <= 8192);
    }
}
