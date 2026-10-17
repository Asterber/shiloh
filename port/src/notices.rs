//! License notices compiled into every binary and the wasm module, so that they ship with them
//! (`xf --licenses`, `xf-serve --licenses` / `/LICENSES.txt`, `xf-prep --licenses`).

/// The project's license line followed by `port/LICENSE-THIRD-PARTY`.
pub const LICENSES: &str = concat!(
    "Shiloh: an engine for The X-Files Game (1998). Not affiliated with or endorsed by the game's rights holders.\n",
    "Licensed under the MIT License or the Apache License, Version 2.0, at your option\n",
    "(LICENSE-MIT, LICENSE-APACHE in the source repository).\n\n",
    include_str!("../LICENSE-THIRD-PARTY"),
);

#[cfg(test)]
mod tests {
    #[test]
    fn notices_carry_the_third_party_texts() {
        assert!(super::LICENSES.contains("Copyright (c) 2026 Zeffuro"));
        assert!(super::LICENSES.contains("Copyright (c) 2021 Advanced Micro Devices"));
        assert!(super::LICENSES.contains("Copyright (C) 1997-2023 Sam Lantinga"));
    }
}
