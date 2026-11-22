//! DirectWrite exposes all localized family names, while GPUI lists one per family.

use std::collections::HashMap;
use windows::{
    Win32::Graphics::DirectWrite::{
        DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
        DWRITE_FONT_WEIGHT_NORMAL, DWriteCreateFactory, IDWriteFactory3, IDWriteFontCollection1,
    },
    core::HSTRING,
};

fn system_fonts() -> Option<IDWriteFontCollection1> {
    // SAFETY: DirectWrite creates the factory and writes a COM interface into
    // the initialized output slot. Both interfaces are owned by the wrappers.
    #[allow(unsafe_code)]
    unsafe {
        let factory: IDWriteFactory3 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
        let mut collection = None;
        factory
            .GetSystemFontCollection(false, &mut collection, false)
            .ok()?;
        collection
    }
}

/// Maps each name to the other names of the same family, all lowercased.
/// Failure leaves the picker usable with the names GPUI already provided.
pub(crate) fn aliases() -> HashMap<String, Vec<String>> {
    let Some(collection) = system_fonts() else {
        return HashMap::new();
    };
    let mut aliases = HashMap::<String, Vec<String>>::new();
    // SAFETY: Each COM interface and buffer stays alive for every call that
    // uses it. DirectWrite fills the allocated UTF-16 buffer including its NUL.
    #[allow(unsafe_code)]
    unsafe {
        for index in 0..collection.GetFontFamilyCount() {
            let Ok(family) = collection.GetFontFamily(index) else {
                continue;
            };
            let Ok(names) = family.GetFamilyNames() else {
                continue;
            };
            let mut family_names = Vec::new();
            for name_index in 0..names.GetCount() {
                let Ok(length) = names.GetStringLength(name_index) else {
                    continue;
                };
                if length > 4096 {
                    continue;
                }
                let mut buffer = vec![0u16; length as usize + 1];
                if names.GetString(name_index, &mut buffer).is_ok()
                    && let Ok(name) = String::from_utf16(&buffer[..length as usize])
                {
                    family_names.push(name.to_lowercase());
                }
            }
            family_names.sort_unstable();
            family_names.dedup();
            if family_names.len() > 1 {
                for name in &family_names {
                    aliases
                        .entry(name.clone())
                        .or_default()
                        .extend(family_names.iter().filter(|other| *other != name).cloned());
                }
            }
        }
    }
    aliases
}

/// Use the same DirectWrite lookup as GPUI before accepting an unlisted name.
pub(crate) fn resolves(family: &str) -> bool {
    let Some(collection) = system_fonts() else {
        return false;
    };
    let name = HSTRING::from(family);
    // SAFETY: The collection and NUL-terminated HSTRING outlive the calls,
    // and the returned COM interfaces are owned by their wrappers.
    #[allow(unsafe_code)]
    unsafe {
        collection
            .GetFontSet()
            .and_then(|set| {
                set.GetMatchingFonts(
                    &name,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                )
            })
            .is_ok_and(|matches| matches.GetFontCount() > 0)
    }
}
