import os
from PIL import Image, ImageDraw

def lerp_color(c1, c2, t):
    return tuple(int(a + (b - a) * t) for a, b in zip(c1, c2))

def create_datara_icon(size):
    # Oversample 4x for smooth antialiasing
    scale = 4
    w = size * scale
    img = Image.new("RGBA", (w, w), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # 1. Background Squircle Monolith
    margin = int(w * 0.035)
    radius = int(w * 0.22)
    draw.rounded_rectangle(
        [margin, margin, w - margin, w - margin],
        radius=radius,
        fill=(14, 16, 20, 255),
        outline=(55, 62, 75, 255),
        width=max(1, int(w * 0.025))
    )

    # Subtle ember corner glow
    glow_p1 = (int(w * 0.75), int(w * 0.60))
    glow_p2 = (w - margin, w - margin)
    # 2. Pipe "|" : Titanium SSA Column
    pipe_x0 = int(w * 0.23)
    pipe_x1 = int(w * 0.36)
    pipe_y0 = int(w * 0.19)
    pipe_y1 = int(w * 0.81)
    pipe_r = max(2, int(w * 0.065))
    
    # Draw pipe vertical gradient
    pipe_mask = Image.new("L", (w, w), 0)
    p_draw = ImageDraw.Draw(pipe_mask)
    p_draw.rounded_rectangle([pipe_x0, pipe_y0, pipe_x1, pipe_y1], radius=pipe_r, fill=255)

    pipe_grad = Image.new("RGBA", (w, w), (0, 0, 0, 0))
    g_draw = ImageDraw.Draw(pipe_grad)
    for y in range(pipe_y0, pipe_y1 + 1):
        t = (y - pipe_y0) / max(1, pipe_y1 - pipe_y0)
        c = lerp_color((245, 248, 255, 255), (125, 138, 155, 255), t)
        g_draw.line([(pipe_x0, y), (pipe_x1, y)], fill=c)
    
    img.paste(pipe_grad, (0, 0), pipe_mask)

    # 3. Arrow ">" : Forgen Kinetic Ember Chevron
    chevron_x0 = int(w * 0.45)
    chevron_tip_x = int(w * 0.78)
    chevron_inner_x = int(w * 0.59)
    chevron_y_top = int(w * 0.19)
    chevron_y_mid = int(w * 0.50)
    chevron_y_bot = int(w * 0.81)
    chevron_y_in_top = int(w * 0.36)
    chevron_y_in_bot = int(w * 0.64)

    # Upper wing (hot golden amber)
    upper_poly = [
        (chevron_x0, chevron_y_top),
        (chevron_tip_x, chevron_y_mid),
        (chevron_inner_x, chevron_y_mid),
        (chevron_x0, chevron_y_in_top),
    ]
    draw.polygon(upper_poly, fill=(255, 120, 0, 255))
    # Bevel on upper wing
    draw.line([(chevron_x0, chevron_y_top), (chevron_tip_x, chevron_y_mid)], fill=(255, 210, 110, 255), width=max(1, int(w * 0.015)))

    # Lower wing (deep fiery ruby amber)
    lower_poly = [
        (chevron_x0, chevron_y_bot),
        (chevron_tip_x, chevron_y_mid),
        (chevron_inner_x, chevron_y_mid),
        (chevron_x0, chevron_y_in_bot),
    ]
    draw.polygon(lower_poly, fill=(215, 45, 0, 255))

    # Center seam line
    draw.line([(chevron_inner_x, chevron_y_mid), (chevron_tip_x, chevron_y_mid)], fill=(15, 18, 22, 255), width=max(1, int(w * 0.018)))

    # Downsample back to target resolution with Lanczos
    return img.resize((size, size), Image.Resampling.LANCZOS)

sizes = [16, 24, 32, 48, 64, 128, 256]
images = [create_datara_icon(s) for s in sizes]

out_ico = os.path.abspath(r"d:\DATARA\datara + forgen\assets\datara.ico")
images[0].save(
    out_ico,
    format="ICO",
    sizes=[(s, s) for s in sizes],
    append_images=images[1:]
)

print(f"Generated multi-resolution ICO file at: {out_ico}")
