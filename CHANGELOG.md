# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-31

### Added
- Real-time Kerr (spinning) black-hole renderer in Bevy 0.19 with Schwarzschild degeneration at spin $\chi = 0$.
- Multi-stage HDR pipeline: sub-resolution offscreen black hole quad, bright-pass filter, 4-stage blur pyramid, and ACES tone mapping.
- CPU ↔ WGSL shader mirror for RK45 geodesic integration with automated unit and integration tests.
- Volumetric accretion disk with blackbody radiation color spectrum and Doppler / gravitational redshift.
- Keplerian planet orbits with Lense-Thirring nodal precession and relativistic light-bending shadow/lensing.
- Interactive egui control panel with instant parameter adjustments, quality presets, and orbit camera navigation.
- Desktop (LTO fat profile) and Web (WebGPU / WebAssembly via Trunk) dual-target support.
