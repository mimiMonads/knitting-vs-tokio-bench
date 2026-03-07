import matplotlib.pyplot as plt

# Gentle dark palette to avoid bright white backgrounds in saved charts.
DARK_BG_HEX = "#0f1116"
DARK_AXES_HEX = "#111318"
DARK_BG_RGB = (15, 17, 22)
TEXT_HEX = "#e6e6e6"
TITLE_HEX = "#f0f0f0"
GRID_HEX = "#3a3f4b"
AXIS_HEX = "#c7c7c7"
LEGEND_BG_HEX = DARK_AXES_HEX
LEGEND_EDGE_HEX = "#2a2f3a"
RUNTIME_ORDER = ("tokio", "bun", "node", "deno")
RUNTIME_LABELS = {
    "tokio": "tokio",
    "bun": "bun",
    "node": "node",
    "deno": "deno",
}
RUNTIME_COLORS = {
    "tokio": "#7dd3fc",
    "bun": "#f59e0b",
    "node": "#a3e635",
    "deno": "#60a5fa",
}

def apply_dark_style():
    try:
        plt.style.use("dark_background")
    except Exception:
        pass
    plt.rcParams.update({
        "figure.facecolor": DARK_BG_HEX,
        "axes.facecolor": DARK_AXES_HEX,
        "axes.edgecolor": AXIS_HEX,
        "axes.labelcolor": TEXT_HEX,
        "axes.titlecolor": TITLE_HEX,
        "xtick.color": TEXT_HEX,
        "ytick.color": TEXT_HEX,
        "text.color": TEXT_HEX,
        "grid.color": GRID_HEX,
        "legend.facecolor": LEGEND_BG_HEX,
        "legend.edgecolor": LEGEND_EDGE_HEX,
        "savefig.facecolor": DARK_BG_HEX,
        "savefig.edgecolor": DARK_BG_HEX,
    })
    return True
