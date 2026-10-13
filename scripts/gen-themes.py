#!/usr/bin/env python3
"""Generate the app's theme files from the Qrate pair (plan 7.1).

`assets/themes/qrate-{dark,light}.json` are the hand-tuned originals (Claude Design canvas
`4c5759a2`). Every other palette is derived from them so the whole set keeps one hierarchy:
each Qrate neutral is placed by where its lightness sits between Qrate's background and
foreground, and the same step is taken between the new palette's background and foreground;
each accent colour is swapped for the palette's own, with hover/active shades taken the way
Qrate takes them. A palette therefore only names nine colours.

    python3 scripts/gen-themes.py          # rewrite assets/themes/*.json
    python3 scripts/gen-themes.py --check  # fail if they are out of date (CI)
"""

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
THEMES = ROOT / "assets" / "themes"

PALETTES = {
    "gruvbox": {
        "label": "Gruvbox",
        "dark": dict(bg="#1d2021", fg="#ebdbb2", primary="#fe8019", danger="#fb4934",
                     success="#b8bb26", warning="#fabd2f"),
        "light": dict(bg="#fbf1c7", fg="#3c3836", primary="#af3a03", danger="#9d0006",
                      success="#79740e", warning="#b57614"),
    },
    "everforest": {
        "label": "Everforest",
        "dark": dict(bg="#2d353b", fg="#d3c6aa", primary="#a7c080", danger="#e67e80",
                     success="#83c092", warning="#dbbc7f"),
        "light": dict(bg="#fdf6e3", fg="#5c6a72", primary="#8da101", danger="#f85552",
                      success="#35a77c", warning="#dfa000"),
    },
}

# Qrate's accent families, by their base colour in each mode.
ACCENTS = ("primary", "danger", "success", "warning")


def parse(hex_colour):
    h = hex_colour.lstrip("#")
    rgb = tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))
    alpha = h[6:8] if len(h) == 8 else ""
    return rgb, alpha


def fmt(rgb, alpha=""):
    return "#" + "".join(f"{max(0, min(255, round(c))):02x}" for c in rgb) + alpha


def lum(rgb):
    return 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]


def mix(a, b, t):
    return tuple(x + (y - x) * t for x, y in zip(a, b))


def derive(template, palette, label, mode):
    colours = template["colors"]
    q_bg, _ = parse(colours["background"])
    q_fg, _ = parse(colours["foreground"])
    p_bg, _ = parse(palette["bg"])
    p_fg, _ = parse(palette["fg"])

    # Qrate's accent bases and the derived shades that hang off them.
    q_accents = {name: parse(colours[f"{name}.background"])[0] for name in ACCENTS}

    def swap(value):
        rgb, alpha = parse(value)
        for name, base in q_accents.items():
            if rgb == base:
                return fmt(parse(palette[name])[0], alpha)
        # A hover/active shade: the same move from the palette's own base, for the nearest
        # accent (light danger-hover sits close to the rust primary too).
        distance, name = min(
            (sum(abs(x - y) for x, y in zip(rgb, base)), name) for name, base in q_accents.items()
        )
        if distance < 90:
            base = q_accents[name]
            p_base = parse(palette[name])[0]
            delta = (lum(rgb) - lum(base)) / 255
            target = (255, 255, 255) if delta > 0 else (0, 0, 0)
            return fmt(mix(p_base, target, abs(delta) * 1.2), alpha)
        if rgb in ((255, 255, 255), (0, 0, 0)):
            return value
        # A neutral: the same place between background and foreground.
        span = lum(q_fg) - lum(q_bg)
        t = (lum(rgb) - lum(q_bg)) / span if span else 0
        return fmt(mix(p_bg, p_fg, t), alpha)

    theme = {k: v for k, v in template.items() if k != "colors"}
    theme["name"] = f"{label} {'Dark' if mode == 'dark' else 'Light'}"
    theme["is_default"] = False
    theme["colors"] = {key: swap(value) for key, value in colours.items()}
    # Text on an accent: the palette's background reads on every palette's accent.
    for name in ACCENTS:
        key = f"{name}.foreground"
        if key in theme["colors"]:
            theme["colors"][key] = palette["bg"]
    return theme


def theme_set(theme):
    return {"name": theme["name"], "author": "Islandora Workbench GUI", "themes": [theme]}


def render(obj):
    return json.dumps(obj, indent=2, ensure_ascii=False) + "\n"


def main():
    check = "--check" in sys.argv
    stale = []
    for mode in ("dark", "light"):
        template = json.loads((THEMES / f"qrate-{mode}.json").read_text())["themes"][0]
        for slug, palette in PALETTES.items():
            out = THEMES / f"{slug}-{mode}.json"
            text = render(theme_set(derive(template, palette[mode], palette["label"], mode)))
            if check:
                if not out.exists() or out.read_text() != text:
                    stale.append(out.name)
            else:
                out.write_text(text)
    if stale:
        sys.exit(f"out of date: {', '.join(stale)} — run scripts/gen-themes.py")


if __name__ == "__main__":
    main()
