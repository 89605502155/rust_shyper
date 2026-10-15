#!/bin/bash
# Run inside the MVM: desktop (Xfce on Xorg) in the QEMU window.
#
#   /root/mvm-desktop.sh install   once: install Xorg and Xfce with dnf over
#                                  the QEMU network (needs about 1 GB)
#   /root/mvm-desktop.sh           start the desktop
#
# The hypervisor must run with GUI=1 (tools/qemu-run.sh): the MVM then has a
# framebuffer (/dev/fb0, emulated PL111 shown by QEMU ramfb) and the QEMU USB
# keyboard and tablet. The MVM has no udev, so Xorg gets its devices from a
# generated xorg.conf instead of hotplug.
set -e

PACKAGES="xorg-x11-server-Xorg xorg-x11-drv-evdev xorg-x11-xinit
    xorg-x11-xauth xfce4-session xfwm4 xfce4-panel xfdesktop xfce4-terminal
    xfce4-settings Thunar dbus-x11 dejavu-sans-fonts dejavu-sans-mono-fonts"
# The MVM kernel (Linux 4.9) has no DRM driver for the display, only fbdev.
# Fedora retired the Xorg fbdev driver after Fedora 40, so its last source
# package is rebuilt here against the Xorg of this release.
FBDEV_SRPM=https://archives.fedoraproject.org/pub/archive/fedora/linux/releases/40/Everything/source/tree/Packages/x/xorg-x11-drv-fbdev-0.5.0-15.fc40.src.rpm
CONF=/etc/X11/xorg.conf.d/10-trosna.conf
RPM_CACHE=/var/cache/trosna

event_of() { # <part of the device name>: /dev/input/eventN of an input device
    awk -v name="$1" '
        /^N: / { found = index($0, name) > 0 }
        /^H: / && found { for (i = 2; i <= NF; i++) if ($i ~ /event[0-9]+$/) { sub(/.*=/, "", $i); print "/dev/input/" $i; exit } }
    ' /proc/bus/input/devices
}

build_fbdev() { # rebuild the fbdev driver from FBDEV_SRPM and install it
    local build ok=0
    build=$(mktemp -d)
    rpm -qa --qf '%{NAME}\n' | sort > "$build/before"
    curl -fsSL -o "$build/fbdev.src.rpm" "$FBDEV_SRPM"
    rpm -i --define "_topdir $build" "$build/fbdev.src.rpm"
    # %patchN is an error since RPM 4.20, no package provides "Xorg" any more,
    # and the Xorg of Fedora 42 obsoletes xorg-x11-drv-fbdev < 0.5.0-19.
    sed -i -E -e 's/^%patch([0-9]+)/%patch -P \1/' \
        -e 's/^Requires:[[:space:]]*Xorg.*/Requires: xorg-x11-server-Xorg/' \
        -e 's/^Release:.*/Release: 19.trosna%{?dist}/' "$build"/SPECS/*.spec
    dnf5 -y install --setopt=install_weak_deps=False rpm-build dnf5-plugins
    if dnf5 -y builddep --setopt=install_weak_deps=False "$build"/SPECS/*.spec &&
        rpmbuild --define "_topdir $build" -bb "$build"/SPECS/*.spec; then
        mkdir -p "$RPM_CACHE"
        cp "$build"/RPMS/*/xorg-x11-drv-fbdev-[0-9]*.rpm "$RPM_CACHE"/
        ok=1
    fi
    # Drop the compiler and the -devel packages again.
    rpm -qa --qf '%{NAME}\n' | sort | comm -13 "$build/before" - |
        grep -vx xorg-x11-drv-fbdev | xargs -r dnf5 -y remove
    rm -rf "$build"
    [ $ok = 1 ]
}

if [ "$1" = install ]; then
    [ -s /proc/net/pnp ] && ln -sf /proc/net/pnp /etc/resolv.conf
    # shellcheck disable=SC2086
    dnf5 -y install --setopt=install_weak_deps=False $PACKAGES
    if ! rpm -q xorg-x11-drv-fbdev >/dev/null; then
        ls "$RPM_CACHE"/xorg-x11-drv-fbdev-[0-9]*.rpm >/dev/null 2>&1 || build_fbdev
        # dnf may not resolve the dependency on the installed Xorg for the
        # local package; the driver is built against exactly that Xorg.
        dnf5 -y install "$RPM_CACHE"/xorg-x11-drv-fbdev-[0-9]*.rpm ||
            rpm -U --nodeps "$RPM_CACHE"/xorg-x11-drv-fbdev-[0-9]*.rpm
    fi
    dnf5 clean all
    # The packages bring systemd, which takes over /sbin/init; the MVM keeps
    # its own init (tools/mkrootfs.sh).
    if [ -x /usr/libexec/trosna-init ]; then
        ln -sf ../libexec/trosna-init /usr/sbin/init
    fi
    exit
fi

if [ ! -c /dev/fb0 ]; then
    echo "no /dev/fb0: start the hypervisor with GUI=1 (see tools/qemu-run.sh)" >&2
    exit 1
fi
KBD=$(event_of "QEMU USB Keyboard")
TABLET=$(event_of "QEMU USB Tablet")
if [ -z "$KBD" ] || [ -z "$TABLET" ]; then
    echo "no USB keyboard/tablet in the MVM (GUI=1 adds them)" >&2
    exit 1
fi

mkdir -p "$(dirname $CONF)"
cat > $CONF <<EOF
Section "ServerFlags"
    Option "AutoAddDevices" "false"
    Option "AutoAddGPU" "false"
EndSection
Section "Device"
    Identifier "fb"
    Driver "fbdev"
    Option "fbdev" "/dev/fb0"
EndSection
Section "InputDevice"
    Identifier "kbd"
    Driver "evdev"
    Option "Device" "$KBD"
EndSection
Section "InputDevice"
    Identifier "tablet"
    Driver "evdev"
    Option "Device" "$TABLET"
EndSection
Section "ServerLayout"
    Identifier "trosna"
    InputDevice "kbd" "CoreKeyboard"
    InputDevice "tablet" "CorePointer"
EndSection
EOF

# The MVM init starts no services: the system bus is started here, and the
# PolicyKit agent (root needs none, and polkitd is not running) is disabled.
if [ ! -S /run/dbus/system_bus_socket ]; then
    mkdir -p /run/dbus
    dbus-daemon --system --fork
fi
mkdir -p /root/.config/autostart
printf '[Desktop Entry]\nHidden=true\n' > /root/.config/autostart/xfce-polkit.desktop

export HOME=/root XDG_RUNTIME_DIR=/run/user/0
mkdir -p -m 0700 $XDG_RUNTIME_DIR
exec startx /usr/bin/dbus-launch --exit-with-session /usr/bin/startxfce4 -- :0 vt2 -nolisten tcp
