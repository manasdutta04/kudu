# dmgbuild settings for Kudu-macOS.dmg: the Finder window people see when they open the disk
# image. Run through macos/scripts/make-dmg.sh, which passes the paths with -D.
#
# The window shows 660 x 420 points and the icon centres match the background, which
# macos/scripts/make-dmg-background.swift draws.
import os.path

app = defines["app"]  # noqa: F821 (dmgbuild provides `defines`)
packaging = defines["packaging"]  # noqa: F821
app_name = os.path.basename(app)

format = "UDZO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
icon = defines["volume_icon"]  # noqa: F821
hide_extensions = [app_name]

background = os.path.join(packaging, "dmg-background.tiff")
# Finder counts the title bar in the height: 420 visible points plus about 28.
window_rect = ((200, 160), (660, 448))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
show_icon_preview = False
include_icon_view_settings = True

arrange_by = None
grid_spacing = 100
label_pos = "bottom"
text_size = 13
icon_size = 112
icon_locations = {
    app_name: (170, 196),
    "Applications": (490, 196),
}
