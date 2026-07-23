# Ashkarr system map textures

Each body has a 768 × 384 equirectangular pair:

- `*_surface.png` — environment art with the exact authored province borders.
- `*_province_ids.png` — lossless RGB ID texture for picking. It contains no
  outlines, transparency, filtering, or anti-aliasing.
- `*_province_ids.json` — province key to RGB lookup table.

Ashkarr contains 32 ID colours; Vesk contains 8. Rebuild the assets after a
province coordinate change with `powershell -File tools/build_map_textures.ps1`.
