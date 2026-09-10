import base64
from PIL import Image, ImageDraw, ImageEnhance

def create_badge(size=512):
    img = Image.new('RGBA', (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    s = size / 256.0
    
    def sc(x, y):
        return (x * s, y * s)
    def sc_pts(pts):
        return [sc(p[0], p[1]) for p in pts]
        
    # 1. Dark Carbon Squircle Base with Glowing Electric Amber Border
    bg_dark = (16, 18, 22, 255)
    border_outer = (255, 125, 0, 255)
    border_inner = (255, 195, 20, 255)
    
    # Outer glowing border (radius 58)
    d.rounded_rectangle([sc(4, 4), sc(252, 252)], radius=58*s, fill=border_outer)
    # Inner gold contour
    d.rounded_rectangle([sc(8, 8), sc(248, 248)], radius=54*s, fill=border_inner)
    # Carbon monolith core
    d.rounded_rectangle([sc(12, 12), sc(244, 244)], radius=50*s, fill=bg_dark)
    
    # Colors for the Spark (Maximum vibrancy, saturation, and luminance)
    c_white = (255, 255, 255, 255)
    c_gold = (255, 215, 0, 255)
    c_amber = (255, 140, 10, 255)
    c_fire = (255, 70, 10, 255)
    
    # 2. Vertical Anchor Pillar | (Bold, x: 92..112, y: 110..215)
    d.rounded_rectangle([sc(90, 110), sc(112, 215)], radius=9*s, fill=c_amber)
    
    # 3. Pivot Hub (White hot center)
    d.ellipse([sc(90, 100), sc(114, 124)], fill=c_white)
    
    # 4. Ray 1: Leftward Shard (~10 o'clock)
    d.polygon(sc_pts([(92, 106), (46, 88), (64, 70), (98, 98)]), fill=c_fire)
    
    # 5. Ray 2: Upper-Left Dart (~11 o'clock)
    d.polygon(sc_pts([(96, 100), (75, 38), (98, 48), (104, 95)]), fill=c_gold)
    
    # 6. Ray 3: Vertical Needle (~12 o'clock)
    d.polygon(sc_pts([(103, 95), (120, 28), (136, 46), (110, 95)]), fill=c_fire)
    
    # 7. Ray 4: Upper-Right Dart (~1 o'clock)
    d.polygon(sc_pts([(110, 98), (166, 36), (182, 54), (118, 104)]), fill=c_gold)
    
    # 8. Ray 5: Signature Vector Arrow (~2 o'clock, main pipeline flow)
    d.polygon(sc_pts([(114, 104), (195, 60), (208, 74), (122, 112)]), fill=c_fire)
    d.polygon(sc_pts([(224, 38), (184, 64), (204, 86)]), fill=c_fire)
    
    # 9. Ray 6: Horizontal Forward Chevron > (~3 o'clock)
    d.polygon(sc_pts([(118, 118), (224, 114), (200, 142), (158, 132), (118, 126)]), fill=c_gold)
    
    # 10. Ray 7: Lower-Right Shard (~4 o'clock)
    d.polygon(sc_pts([(116, 130), (208, 160), (186, 186), (148, 166), (114, 136)]), fill=c_fire)
    
    # 11. Ray 8: Bottom Anchor Shard (~5 o'clock)
    d.polygon(sc_pts([(110, 138), (172, 204), (148, 218), (106, 144)]), fill=c_gold)
    
    return img

# Generate high-res badge (512x512)
badge = create_badge(512)
badge.save("assets/icon.png", "PNG")
badge.save("assets/datara-logo.png", "PNG")

# Multi-resolution ICO for Windows Explorer
sizes = [16, 24, 32, 48, 64, 128, 256]
images = [badge.resize((s, s), Image.Resampling.LANCZOS) for s in sizes]
images[0].save(
    "assets/datara.ico",
    format="ICO",
    sizes=[(s, s) for s in sizes],
    append_images=images[1:]
)

# Convert to embedded SVG
with open("assets/icon.png", "rb") as f:
    b64 = base64.b64encode(f.read()).decode("ascii")

svg_content = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="100%" height="100%">
  <image href="data:image/png;base64,{b64}" width="512" height="512" />
</svg>"""

for path in [
    "icon.svg",
    "assets/icon.svg",
    "assets/datara-logo.svg",
    "editors/vscode/icon.svg",
    "editors/vscode/icons/icon.svg",
]:
    with open(path, "w", encoding="utf-8") as f:
        f.write(svg_content)
    print(f"Updated SVG: {path}")

# Also copy PNGs to editors/vscode/
badge.save("editors/vscode/icon.png")
badge.save("editors/vscode/icons/icon.png")

print("All badge assets generated successfully!")
