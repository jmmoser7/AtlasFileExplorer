# Tool icon rules — proposal

Status: Quiet outline and primary icons approved for implementation, 2026-09-15.
The active development specification is `crates/atlas-shell/ICONS.md`; this document
records the design proposal and alternatives. Existing palette interactions remain unchanged.
Scope: tool palettes, flyouts, and corresponding command icons in File Atlas and Slate.

## Decision proposed

Adopt **A / Quiet outline**: 1.5-unit strokes on a 24-unit grid, with a solid Select arrow and hollow Direct select arrow. B / Bold silhouette is the alternative: 1.75-unit strokes and selectively filled object silhouettes. Pick one system before rollout.

The drawings in `proposal.html` are original vector studies, inspired by Figma's restrained monochrome vocabulary. They are review specimens, not production-ready replacements for every existing icon. We reviewed `apps/slate/src/app/board_icons.rs` and the icon API in `crates/atlas-shell/src/dock.rs`.

## Geometry

1. Use a 24 × 24 design grid with a nominal 20 × 20 optical footprint. Keep 2 units of padding; optical exceptions must be recorded with the glyph.
2. Use the chosen family's stroke weight consistently. Round caps and joins are the default; retain sharp points when they carry meaning, especially the selection cursor.
3. Keep internal gaps at least 2 units at the master size. At 16 px, simplify secondary detail; do not shrink a complex illustration until it turns to noise.
4. Balance apparent weight and centering, not just bounding boxes. Circles, diagonals, and the hand require optical review alongside rectangles.
5. Use vectors for production. Store normalized geometry once, with stable semantic IDs and reviewed optical adjustments. No generated raster tool glyphs, emoji, gradients, or decorative shadows.

## Meaning

6. Each glyph gets one primary metaphor. Frame uses extending rails; Rectangle is a closed shape. Arc is curved; Polyline has corners; Bezier has control handles. Pen is a nib.
7. Preserve the solid Select / hollow Direct select pair. Filled versus outlined is a semantic choice, not a blanket active-state transformation.
8. Related operations need distinct structure. Trim indicates removal, Split indicates separation, Join indicates connected endpoints. Run a recognition review with these adjacent and unlabeled before accepting the metaphors.
9. Use the same icon for the same operation across palettes, menus, and apps. Derive tooltips and names from the command owner; retain labels for unfamiliar portal and snap subtypes.
10. Avoid compound miniatures and arbitrary badges. New glyphs need a short explanation of their metaphor and the nearest potentially confusable siblings.

## State and accessibility

11. Use one neutral ink color by default, sourced from the shared Palette/tokens. Reserve blue for active state. Active also has a visible background shape; never communicate state through hue alone.
12. Keep glyph geometry stable during hover, activation, disabling, and focus. Hover uses a neutral surface; active uses a blue surface and contrasting ink; focus has its own visible outline. Verify at least 3:1 contrast for essential active controls against adjacent colors.
13. Disabled controls use the shared disabled treatment and cannot activate. Pinned/open/associated are existing dock states: preserve their distinct meanings. The proposal does not change hover, pin, click, or flyout contracts.
14. Separate glyph size from hit area. Inspect 16, 20, and 24 px glyphs. Retain the current shared dock sizing during a glyph-only rollout; any new 32 px minimum target is a separate shared-chrome proposal, not a per-icon override.

## Ownership and delivery

15. Constitution Articles X and XII govern: shared icon geometry, metrics, and paint belong in `atlas-shell`. Apps provide command-to-icon mappings. Extend or extract from the existing dock icon owner before introducing a second painter. App-specific icons must obey the same design metrics.
16. Review each new glyph in light and dark themes at 100%, 125%, 150%, and 200% Windows scaling. Inspect idle, hover, active, disabled, and focus states at actual size. Enlarged specimens alone are insufficient.
17. Acceptance packet: semantic ID, command mapping, metaphor, 16/20/24 px specimens, sibling comparison, optical exceptions, and shared-owner location. Add behavior tests only when behavior changes; visual updates need visual review.
18. Roll out in stages: Select/Pan/Frame/Rectangle; drawing families; portals; shared dock controls and snap variants. Inventory every ToolIcon and DockIcon before claiming full coverage. This proposal samples 24 tools, not the full inventory.

## Adoption

### Primary palette icons — proposed extension

The preview now covers all 13 primary DockItems present in the two apps' `ui/tools.rs` arrays at review time: eight Slate launchers and five File Atlas launchers. Visibility remains context-dependent.

- **Frame:** extending rails. **Portals:** an opening inside a boundary.
- **Shapes:** square and circle. **Text:** typographic T.
- **Actions:** editing wrench, covering Trim/Split/Join. Validate confusion with settings during recognition review.
- **Object properties:** a subdivided object, covering appearance and tags.
- **Document settings:** page with adjustment tracks; not a grid-toggle glyph.
- **Selection:** bounded object; distinct from the Select cursor and Fit view.
- **Basic filters:** funnel. **Display settings:** display with arranged content.
- **Mode:** lock for View safety; a future state-aware variant must reflect the real View/Edit state with a label. This is not authorization to enter Edit mode.
- **Workflow:** linked assignment stages. **AI · Cursor:** conversation prompt, independent of provider branding.

Primary glyphs represent the family or panel, not whichever child happens to be first or last used. Keep their identity stable while children change. Draw from the same 24-unit master as child tools; the preview's 44 px primary targets are presentation context, not approved dock dimensions. Shared state styling communicates active/open/pinned separately from the glyph. The highlighted primary icon in the proposal indicates the specimen selected for inspection, not an app state model.

After the user chooses a direction, promote the approved rules to one shared icon specification owned by atlas-shell. Add a short scoped agent rule that points to it rather than copying its contents. Coordinate the geometry extraction with Article XII review if the implementation crosses its review triggers. No `board_*.rs` module is proposed here.

## Sources

- [Figma UI3 rationale](https://www.figma.com/blog/behind-our-redesign-ui3/): consistency, understandable icons, restrained interface, and optional labels.
- [Figma toolbar reference](https://help.figma.com/hc/en-us/articles/360041064174-Access-design-tools-from-the-toolbar): tool vocabulary and toolbar context.

All numeric dimensions and implementation rules above are Atlas / Slate proposals, not claims about Figma's internal icon specification.
