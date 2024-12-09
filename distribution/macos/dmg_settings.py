# dmgbuild settings for ShoopDaLoop
import os
pwd = os.getcwd()

# Volume name
volume_name = "ShoopDaLoop"

# Icon for the volume
icon = "{}/distribution/macos/icon.icns".format(pwd)

# # Background image
# background = "path/to/background.png"  # Update with the actual path if needed

# Window position and size
window_rect = ((200, 120), (800, 400))

# Icon size
icon_size = 100

# Files to include
files = ["{}/shoopdaloop.app".format(pwd)]

# Symlinks to create
symlinks = {"Applications": "/Applications"}

# Icon locations
icon_locations = {
    "shoopdaloop.app": (200, 190),
    "Applications": (600, 185)
}
