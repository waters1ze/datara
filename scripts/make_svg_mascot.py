import base64
from PIL import Image

with open(r"assets\icon.png", "rb") as f:
    b64_data = base64.b64encode(f.read()).decode("ascii")

svg_content = f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="100%" height="100%">
  <image href="data:image/png;base64,{b64_data}" width="512" height="512" />
</svg>"""

paths = [
    r"icon.svg",
    r"assets\icon.svg",
    r"assets\datara-logo.svg",
    r"editors\vscode\icon.svg",
    r"editors\vscode\icons\icon.svg",
]

for p in paths:
    with open(p, "w", encoding="utf-8") as f:
        f.write(svg_content)
    print(f"Updated SVG: {p}")
