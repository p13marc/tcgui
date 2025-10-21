# TC GUI Application Icon

This directory should contain `tcgui.png` - a 64x64 pixel icon for the TC GUI application.

## Creating an Icon

You can create an icon using any graphics editor or command-line tools:

### Using ImageMagick (if available):
```bash
convert -size 64x64 xc:transparent \
  -fill '#2563eb' -draw 'roundrectangle 8,8 56,56 8,8' \
  -fill white -pointsize 24 -gravity center \
  -annotate +0+0 'TC' \
  packaging/icons/tcgui.png
```

### Using GIMP:
1. Create a new 64x64 image
2. Add a blue rounded rectangle background
3. Add white "TC" text in the center
4. Export as PNG

### Using Inkscape:
1. Create a new document (64x64)
2. Draw a blue rounded rectangle
3. Add white "TC" text
4. Export as PNG

The icon will be installed to `/usr/share/pixmaps/tcgui.png` and referenced by the desktop entry.