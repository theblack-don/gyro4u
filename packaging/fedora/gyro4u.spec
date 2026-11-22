%{!?_udevrulesdir:%global _udevrulesdir %{_prefix}/lib/udev/rules.d}
%{!?_userunitdir:%global _userunitdir %{_prefix}/lib/systemd/user}

Name:           gyro4u
Version:        0.2.0
Release:        1%{?dist}
Summary:        DualSense, Steam Controller, and 8BitDo Pro 3 utility

License:        MIT
URL:            https://github.com/theblack-don/gyro4u
Source0:        %{url}/archive/refs/tags/v%{version}.tar.gz#/gyro4u-%{version}.tar.gz

BuildRequires:  rust
BuildRequires:  cargo
BuildRequires:  gcc
BuildRequires:  pkgconf-pkg-config
BuildRequires:  alsa-lib-devel
BuildRequires:  openssl-devel
BuildRequires:  systemd-devel
BuildRequires:  libxkbcommon-devel
BuildRequires:  libX11-devel
BuildRequires:  libXcursor-devel
BuildRequires:  libXi-devel
BuildRequires:  libXrandr-devel
BuildRequires:  libXrender-devel
BuildRequires:  libxcb-devel
BuildRequires:  libglvnd-devel
BuildRequires:  vulkan-loader-devel
BuildRequires:  wayland-devel
BuildRequires:  clang
BuildRequires:  lld

%description
Gyro4U is a native Linux GUI utility for configuring DualSense, DualSense Edge,
Steam Controller (2026), and 8BitDo Pro 3 controllers: input remapping, layers,
trackpad scrolling and haptics, gyro-to-mouse, stick curves, adaptive triggers,
profiles, and DualSense firmware updates. 8BitDo support covers wired or
Bluetooth DInput, extra buttons, basic rumble, and battery; Bluetooth is
experimental, the 2.4G receiver is not supported, and lightbar, audio, firmware,
adaptive triggers, and touchpad features are unavailable.

%prep
%autosetup -n gyro4u-%{version}

%build
cargo build --release --locked

%install
install -Dm755 target/release/gyro4u %{buildroot}%{_bindir}/gyro4u

install -Dm644 assets/icon.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/gyro4u.svg

install -Dm644 assets/gyro4u.desktop \
    %{buildroot}%{_datadir}/applications/gyro4u.desktop

install -Dm644 assets/70-gyro4u.rules \
    %{buildroot}%{_udevrulesdir}/70-gyro4u.rules

install -Dm644 assets/gyro4u.conf \
    %{buildroot}%{_prefix}/lib/modules-load.d/gyro4u.conf

install -Dm644 assets/gyro4u-daemon.service \
    %{buildroot}%{_userunitdir}/gyro4u-daemon.service

install -Dm644 LICENSE %{buildroot}%{_licensedir}/gyro4u/LICENSE

%files
%license %{_licensedir}/gyro4u/LICENSE
%{_bindir}/gyro4u
%{_datadir}/icons/hicolor/scalable/apps/gyro4u.svg
%{_datadir}/applications/gyro4u.desktop
%{_udevrulesdir}/70-gyro4u.rules
%{_prefix}/lib/modules-load.d/gyro4u.conf
%{_userunitdir}/gyro4u-daemon.service

%changelog
* Thu Nov 19 2026 theblack-don <theblack-don@users.noreply.github.com> - 0.2.0-1
- Initial Gyro4U package
