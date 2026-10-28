# HiTech-OS — universal live/target hardware support.
#
# The first physical tests showed that the minimal installer firmware set
# hid Wi-Fi adapters and left audio/GPU support incomplete. This module is
# intentionally shared by the NetInstaller, the Live ISO, and installed
# targets so the same hardware works before and after installation.
{ config, lib, pkgs, ... }:

{
  # Firmware packages such as Broadcom/Intel wireless blobs and some GPU
  # microcode are unfree but redistributable; allow them explicitly.
  nixpkgs.config.allowUnfree = true;

  # Pull the complete linux-firmware + vendor firmware closures, including
  # Wi-Fi, Bluetooth, audio DSP, and GPU firmware.
  hardware.enableAllFirmware = true;
  hardware.enableRedistributableFirmware = true;
  hardware.wirelessRegulatoryDatabase = true;
  hardware.firmware = with pkgs; [
    linux-firmware
    sof-firmware
    alsa-firmware
  ];

  # Load the common wireless/audio/GPU drivers even when udev coldplug would
  # otherwise defer them until after the installer starts.
  boot.kernelModules = [
    # Intel Wi-Fi
    "iwlwifi"
    "iwlmvm"
    # Realtek
    "rtw88_core"
    "rtw88_pci"
    "rtw89_core"
    "rtw89_pci"
    "rtl8xxxu"
    "rtl8188eu"
    "rtl8821ce"
    # MediaTek
    "mt76"
    "mt76x2e"
    "mt7921e"
    "mt7601u"
    # Broadcom / Atheros
    "brcmfmac"
    "brcmsmac"
    "ath10k_pci"
    "ath11k_pci"
    "ath12k"
    # Audio
    "snd_hda_intel"
    "snd_hda_codec_realtek"
    "snd_hda_codec_hdmi"
    "snd_usb_audio"
    # GPU / DRM
    "amdgpu"
    "radeon"
    "i915"
    "xe"
    "nouveau"
    "virtio_gpu"
  ];
  # GPU drivers are deliberately NOT staged into the initrd. Including them
  # pulls every matching firmware blob as well, which produced a 1.44 GiB
  # initrd and a >4 GiB ISO whose initrd ISOLINUX could no longer load on the
  # BIOS path ("Loading initrd...failed: Invalid argument"). They are listed in
  # boot.kernelModules above and are loaded by udev at stage 2, which is early
  # enough for the kiosk/desktop session.

  # common.nix blacklists audio/GPU for the headless patient-zero profile;
  # explicit hardware support wins on installer/live images and desktops.
  boot.blacklistedKernelModules = lib.mkForce [ ];
  hardware.graphics.enable = true;

  # Sound stack: PipeWire + WirePlumber with ALSA/Pulse compatibility.
  services.pipewire = {
    enable = true;
    alsa.enable = true;
    alsa.support32Bit = true;
    pulse.enable = true;
    wireplumber.enable = true;
  };

  security.polkit.enable = lib.mkForce true;
  environment.systemPackages = with pkgs; [
    gparted
    polkit
    pipewire
    wireplumber
    alsa-utils
    pavucontrol
    pciutils
    usbutils
  ];
}
