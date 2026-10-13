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


def relative_luminance(rgb):
    """WCAG 2.x relative luminance."""
    def channel(c):
        c = c / 255
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = (channel(c) for c in rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a, b):
    la, lb = sorted((relative_luminance(a), relative_luminance(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


AA = 4.5  # WCAG AA for body text


def readable_accent(base, candidates, mode):
    """The accent, darkened (light mode) or lightened (dark mode) just enough that one of the
    candidate text colours reaches AA on it, and that text colour."""
    toward = (0, 0, 0) if mode == "light" else (255, 255, 255)
    for step in range(0, 41):
        accent = mix(base, toward, step * 0.02)
        accent = tuple(round(c) for c in accent)
        text = max(candidates, key=lambda t: contrast(t, accent))
        if contrast(text, accent) >= AA:
            return accent, text
    return accent, text


def readable_neutral(rgb, bg, fg):
    """Move a text neutral toward the foreground until it reaches AA on the background."""
    for step in range(0, 51):
        candidate = tuple(round(c) for c in mix(rgb, fg, step * 0.02))
        if contrast(candidate, bg) >= AA:
            return candidate
    return fg


# Neutrals used as text, which must stay readable on the background.
TEXT_NEUTRALS = ("muted.foreground", "tab.foreground", "table.head.foreground")


def derive(template, palette, label, mode):
    colours = template["colors"]
    q_bg, _ = parse(colours["background"])
    q_fg, _ = parse(colours["foreground"])
    p_bg, _ = parse(palette["bg"])
    p_fg, _ = parse(palette["fg"])

    # Qrate's accent bases, and the palette's own — each nudged until text on it is readable.
    q_accents = {name: parse(colours[f"{name}.background"])[0] for name in ACCENTS}
    candidates = [p_bg, p_fg, (0, 0, 0), (255, 255, 255)]
    accents, accent_text = {}, {}
    for name in ACCENTS:
        accents[name], accent_text[name] = readable_accent(
            parse(palette[name])[0], candidates, mode
        )

    def shade(name, rgb, alpha):
        """A hover/active shade: Qrate's move from its base, taken from the palette's base."""
        base = q_accents[name]
        delta = (lum(rgb) - lum(base)) / 255
        target = (255, 255, 255) if delta > 0 else (0, 0, 0)
        return fmt(mix(accents[name], target, abs(delta) * 1.2), alpha)

    def swap(key, value):
        rgb, alpha = parse(value)
        # An accent key belongs to its own family by name (a pressed danger button is red even
        # where Qrate's red-pressed is nearer its rust primary).
        family = key.split(".")[0]
        if family in ACCENTS and key.endswith("background"):
            if rgb == q_accents[family]:
                return fmt(accents[family], alpha)
            return shade(family, rgb, alpha)
        for name, base in q_accents.items():
            if rgb == base:
                return fmt(accents[name], alpha)
        # Other keys drawn in an accent (ring, link, selection, slider…): the nearest family.
        distance, name = min(
            (sum(abs(x - y) for x, y in zip(rgb, base)), name) for name, base in q_accents.items()
        )
        if distance < 90:
            return shade(name, rgb, alpha)
        if rgb in ((255, 255, 255), (0, 0, 0)):
            return value
        # A neutral: the same place between background and foreground.
        span = lum(q_fg) - lum(q_bg)
        t = (lum(rgb) - lum(q_bg)) / span if span else 0
        return fmt(mix(p_bg, p_fg, t), alpha)

    theme = {k: v for k, v in template.items() if k != "colors"}
    theme["name"] = f"{label} {'Dark' if mode == 'dark' else 'Light'}"
    theme["is_default"] = False
    theme["colors"] = {key: swap(key, value) for key, value in colours.items()}
    for name in ACCENTS:
        key = f"{name}.foreground"
        if key in theme["colors"]:
            theme["colors"][key] = fmt(accent_text[name])
    for key in TEXT_NEUTRALS:
        if key in theme["colors"]:
            rgb, alpha = parse(theme["colors"][key])
            theme["colors"][key] = fmt(readable_neutral(rgb, p_bg, p_fg), alpha)
    return theme


def contrast_problems(theme):
    """Text/surface pairs below AA in a generated theme."""
    c = {k: parse(v)[0] for k, v in theme["colors"].items()}
    pairs = [(f"{n}.foreground", f"{n}.background") for n in ACCENTS]
    pairs += [(k, "background") for k in TEXT_NEUTRALS] + [("foreground", "background")]
    return [
        f"{theme['name']}: {fg} on {bg} is {contrast(c[fg], c[bg]):.2f}"
        for fg, bg in pairs
        if fg in c and bg in c and contrast(c[fg], c[bg]) < AA
    ]


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
            theme = derive(template, palette[mode], palette["label"], mode)
            stale.extend(contrast_problems(theme))
            text = render(theme_set(theme))
            if check:
                if not out.exists() or out.read_text() != text:
                    stale.append(out.name)
            else:
                out.write_text(text)
    if stale:
        sys.exit("problems: " + "; ".join(stale) + " — fix the palette or run scripts/gen-themes.py")


if __name__ == "__main__":
    main()
