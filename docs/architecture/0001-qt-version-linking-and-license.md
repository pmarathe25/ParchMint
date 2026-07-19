# ADR-0001: Qt version, modules, and linking posture

Status: Accepted (Stage 01)

## Decision

Develop and test with Qt 6.8.3 from the Qt 6.8 LTS line. Use Core, Gui, Qml,
Quick, Quick Controls 2, Sql, Test, and QuickTest. Link Qt dynamically. Use a
restrained Material style through shared QML design tokens.

Do not create release artifacts, statically link Qt, or claim a distribution
license until a later user-approved licensing ADR resolves the open product
decision. CI smoke artifacts are non-release engineering evidence only.

## Consequences

The native stack remains stable across all three target operating systems and
uses Qt under its applicable development terms. Qt patch upgrades require the
full cross-platform foundation suite; changing the minor line requires a new
ADR. Dependency notices and license inventory begin before packaging.
