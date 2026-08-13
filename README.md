# ParchMint

ParchMint is a local-first desktop application for planning and writing novels
on Windows, macOS, and Linux.

## Start here

Read these documents in this order:

1. [Product specification](docs/product/README.md) — what the
   v1 product does and what is outside its scope.
2. [Architecture](docs/architecture/architecture.md) — how the application is
   split into crates, where data lives, and how the crates work together.
3. [UI design](docs/ui-design/README.md) — the visual language, layouts, and
   presentation of the product.
4. [Future work](docs/product/future-work.md) — ideas that are outside v1.

The [decisions log](docs/decisions.md) gives short explanations for technical
choices that affect the whole codebase. It provides context; it does not add
requirements or override the other documents.

## Which document wins?

When the maintained documents disagree, use this order:

1. The product specification controls observable behavior and v1 scope.
2. The architecture controls ownership, boundaries, canonical formats, and
   selected technology.
3. The UI design documentation controls presentation where it does not change
   product behavior.

Update the applicable current document when an approved decision changes it.

## License

ParchMint is free software licensed under the
[GNU General Public License, version 3 or later](LICENSE).
