/* Canonical offline catalog. Exported results include this complete snapshot. */
window.SelectionAcceptanceCatalog = {
  id: 'slate-selection-regions-2026-10-20', revision: 1, created: '2026-10-20',
  title: 'Slate selection regions — exact-build native acceptance',
  source: 'docs/keymap/contracts/selection-hit-regions.md, GP1–GP9; approved user sandbox iteration of 20 October 2026',
  nativeResultsPrefilled: false,
  cases: [
    {
      id: 'GP1', title: 'Zoom from normal objects to points and back', category: 'current', initialStatus: 'untested',
      concern: 'Small objects should gradually favor moving, while zooming in makes fine actions usable again.',
      setup: [
        'Create a disposable board with four separate rectangular samples: Normal 240×112, Small 96×48, Extra small 24×18, Point 1×1 world units. Include a wire-capable image/rectangle and a rotatable host.',
        'Record theme, display scale, actual sample dimensions and starting zoom. If the app cannot author a required size, record that limit as Blocked; do not silently replace the 1×1 sample with an easier one.'
      ],
      actions: [
        'At a usable zoom, try the body, an edge, a corner, rotation and a wire port. Then zoom out gradually with the wheel over the sample, pausing at several intermediate sizes.',
        'At the smallest useful view, drag from the small object to move it. Zoom back in and repeat the fine operations on the same object, including the 1×1 host at the supported high zoom.',
        'Compare two different world-size objects at approximately the same visible size by changing zoom. Record the zooms and any sudden switch, flicker, missed target, or accidental operation.'
      ],
      checks: [
        {id: 'GP1.1', text: 'On zoom-out, wire relevance recedes first, resize/fillet next, rotation next, while move remains usable. The transition feels progressive rather than jumping to unrelated targets.'},
        {id: 'GP1.2', text: 'The tiny/point-like body can be moved without an accidental resize, rotation or wire. Dormant fine controls do not intercept its press.'},
        {id: 'GP1.3', text: 'Zooming back in restores the eligible fine actions on the same small authored object, including wires. An authored-size cutoff does not permanently remove them.'},
        {id: 'GP1.4', text: 'Different world-size samples at equivalent projected size have comparable hit reach. The compact frame does not visibly enlarge the object itself.'},
        {id: 'GP1.5', text: 'Each completed manipulation has one undo step; Undo restores the previous geometry and does not leave a stale control frame.'}
      ]
    },
    {
      id: 'GP2', title: 'Use whole edges and rotation arcs, not isolated points', category: 'current', initialStatus: 'untested',
      concern: 'Hit areas should respond to pointer proximity across an entire relevant feature.',
      setup: ['Use a large rotatable rectangle or image with no overlapping neighbors. Keep its edges and exterior corners away from window chrome. Record zoom and size.'],
      actions: [
        'Hover and start drags at roughly one-quarter, one-half and three-quarters of a long edge. Release each resize and undo it.',
        'Try rotation at several positions around the same exterior corner arc rather than only its diagonal center. Release and undo each rotation.',
        'Approach the edge/corner slowly from outside, then move along it. Repeat on a compact object at another zoom. Note where the cursor starts announcing each action.'
      ],
      checks: [
        {id: 'GP2.1', text: 'All three long-edge locations start the corresponding edge resize; no midpoint-only gap forces a hunt for a small hit point.'},
        {id: 'GP2.2', text: 'The usable exterior rotation area is continuous around the corner and keeps rotation distinct from scaling.'},
        {id: 'GP2.3', text: 'The hover cursor names the operation that actually begins on press. Nearby reach adapts without teleporting the controls into scattered reserve dots.'},
        {id: 'GP2.4', text: 'Moving remains available in the interior. The operation keeps its press owner during the drag, and each completed action undoes once.'}
      ]
    },
    {
      id: 'GP3', title: 'Keep the top object’s move, scale and rotation priority', category: 'current', initialStatus: 'untested',
      concern: 'Adding a lower object must not carve away the foreground object’s familiar interaction areas.',
      setup: [
        'Make a rotatable, corner-capable host B with no neighbor. Capture its usable move core, an edge, a corner, exterior rotation and fillet grip at a fixed zoom.',
        'Add host A behind B, without changing B or the camera. A should extend under B’s body and exterior controls with genuine positive-area overlap, not just touch.'
      ],
      actions: [
        'Repeat the same hover/press locations on B after A is behind it. Complete and undo a move, resize, rotation and fillet adjustment.',
        'Drag where B covers a rear corner of A, then try an exposed edge of A. Reverse front/back order and repeat the conflict.',
        'Repeat with a compact top object and a rotated top object. Capture any newly dead region or wrong-host gesture.'
      ],
      checks: [
        {id: 'GP3.1', text: 'B’s move, resize, rotation and fillet zones keep their isolated reach and choose B while it is on top; a lower body or grip does not shrink or steal them.'},
        {id: 'GP3.2', text: 'A press over a covered rear corner manipulates the appropriate foreground B region. Exposed rear geometry remains usable where B has no priority region.'},
        {id: 'GP3.3', text: 'Reversing stacking order reverses foreground priority. Compact and rotated variants obey the same rule.'},
        {id: 'GP3.4', text: 'The object and action announced on hover are those actually changed. The committed result is visible and one Undo restores it.'}
      ]
    },
    {
      id: 'GP4', title: 'Start overlapping-object wires from inside the top host', category: 'current', initialStatus: 'untested',
      concern: 'Outside wire reach should not steal the top object’s movement and scaling, while its interior port remains usable.',
      setup: [
        'Overlap two wire-capable hosts with B in front; expose B’s port edge over the body of A. Include a separate target for a completed connection.',
        'Prepare a compact front host, a rotated or rounded front host, and a normal-height image/generator pair to check the native flow rail.'
      ],
      actions: [
        'Approach B’s port from just outside its actual silhouette and press-drag. Undo. Repeat from just inside its edge at the port and connect to the separate target.',
        'Repeat on the compact front host and rotated/rounded edge. If the wire domain is dormant, zoom in before trying the eligible port.',
        'Check a normal image-to-generator connection and a short-host connection, then Undo/Redo, save/reopen a copy and export an HTML artifact to inspect their attachment sites.'
      ],
      checks: [
        {id: 'GP4.1', text: 'An outside press at B’s overlapped port does not start its wire; the appropriate move/scale action retains the area.'},
        {id: 'GP4.2', text: 'A strictly interior eligible-port press starts B’s wire, including compact and rotated/rounded variants. The actual visible silhouette is respected.'},
        {id: 'GP4.3', text: 'Short-host ports return when large enough on screen and stay attached to their host; normal-height train rails retain their usual alignment and input roles.'},
        {id: 'GP4.4', text: 'The correct endpoints and input roles survive one-step undo/redo and save/reopen. The exported connection attaches consistently and contains no editing control frame.'}
      ]
    },
    {
      id: 'GP5', title: 'Break touching-corner ties by the nearer host geometry', category: 'current', initialStatus: 'untested',
      concern: 'Two coincident corner hit areas should follow which host the pointer is actually nearer, rather than always choosing front order.',
      setup: [
        'Place two rectangles directly next to each other with their upper meeting corners exactly coincident and no positive-area overlap. Use native alignment/snaps; record the dimensions and capture the seam.',
        'If exact contact cannot be established, record the setup as Blocked. A gap or slight overlap cannot verify the adjacency tie rule.'
      ],
      actions: [
        'At the shared corner, press slightly toward A’s actual outline and resize; undo. Press slightly toward B’s outline and resize; undo. Test the exact equal-distance location too.',
        'Reverse front order while retaining contact and repeat. Test at approximately 50%, 100%, 200% and 400%, or record unavailable zooms.',
        'Repeat with rotated and rounded hosts whose relevant corners still genuinely coincide. Finally move one object barely across the boundary so the pair overlaps, and compare ownership.'
      ],
      checks: [
        {id: 'GP5.1', text: 'The A-side probe chooses A and the B-side probe chooses B even after front order is reversed.'},
        {id: 'GP5.2', text: 'At genuinely equal distances the front host wins deterministically; slight pointer motion to a nearer host changes the winner without an unrelated jump.'},
        {id: 'GP5.3', text: 'The rule follows actual rotated/rounded host geometry across tested zooms, rather than center distance or empty bounding-box corners.'},
        {id: 'GP5.4', text: 'Positive-area overlap uses foreground priority instead of the touching-corner tie rule. Completed drags affect only the intended host and undo once.'}
      ]
    },
    {
      id: 'GP6', title: 'Recover an intentionally selected buried object', category: 'current', initialStatus: 'untested',
      concern: 'A hidden object needs a coherent recovery affordance without putting rear hit points over foreground editing regions.',
      setup: ['Put small host A completely behind a larger host B. Keep B in view. Prepare a way to select A explicitly through the native object/layer selector without first dragging B aside; if that route is unavailable, record the gap.'],
      actions: [
        'With A unselected, hover and manipulate B. Check for floating rear grips or surprising rear actions.',
        'Select the buried A explicitly. Find its outside frame and move affordance. Drag A out into view, release, then Undo.',
        'While A remains selected beneath B, approach B’s move/scale/rotation regions. Test near a viewport edge and with a second covering object.'
      ],
      checks: [
        {id: 'GP6.1', text: 'An unselected fully buried A does not place scattered active hit points over B or steal B’s actions.'},
        {id: 'GP6.2', text: 'Explicitly selecting A exposes one coherent outside frame and a discoverable move affordance clear of foreground bodies and higher-priority transform regions.'},
        {id: 'GP6.3', text: 'The outside affordance moves A into view on the first drag; B stays unchanged. One Undo restores A’s original position.'},
        {id: 'GP6.4', text: 'Foreground B regions still win where appropriate. Any truly covered or clipped affordance is recorded as unavailable, rather than treated as a synthetic reachability success.'}
      ]
    },
    {
      id: 'GP7', title: 'Keep the original press owner, cancellation and selection', category: 'current', initialStatus: 'untested',
      concern: 'A drag must not change tool or target midway, and every manipulation must be reversible as one action.',
      setup: ['Use two overlapping objects, a compact object and a selected multi-object group. Leave space to drag across another host’s fields. Record whether selection or crop mode is active.'],
      actions: [
        'Start resize, rotation, fillet and wire gestures in turn. Cross other fields and another host during each. For resize, shrink through a relevance boundary; use camera zoom during a held gesture where the native command supports it.',
        'For every gesture type, cancel once with Escape before release, then repeat and finish. Undo the finished gesture once and Redo once.',
        'Shift-click an object to toggle selection, then Shift-drag the intended selected group. Repeat without Shift and check the announced action against the actual result.'
      ],
      checks: [
        {id: 'GP7.1', text: 'The original press determines target and operation through field, host and relevance changes. No midway wire/resize/rotate/move substitution occurs.'},
        {id: 'GP7.2', text: 'Escape restores every affected node to its press-time state, adds no undo step and leaves the selection/mode in its existing documented state.'},
        {id: 'GP7.3', text: 'A completed manipulation is reversed by one Undo and reapplied by one Redo. A no-op creates no extra undo entry.'},
        {id: 'GP7.4', text: 'Shift-click toggles selection without an unwanted drag; Shift-drag retains the intended selected group and existing modifier semantics.'}
      ]
    },
    {
      id: 'GP8', title: 'Protect native curve, crop, property and portal editing', category: 'regression', initialStatus: 'untested',
      concern: 'The new common regions must not take input away from existing specialized native editors.',
      setup: ['Prepare a line, Bézier/path, rectangle and polygon with editable vertices, a croppable image, a group, and a nonrotatable portal. Use a local source for the portal; an unavailable source is a blocked variant, not a pass.'],
      actions: [
        'Under Select and Direct Select, pick and drag curve endpoints/handles. Use Ctrl+Shift to pick a curve segment. Try the rectangle/polygon’s explicit vertex and side controls.',
        'Enter crop, pan the image inside its window, and adjust crop edges. Open a property popup and use it, then test a relevant grip press while it is open.',
        'Move a group and resize the portal from its frame. Try the portal’s own body controls. Probe an empty corner of a nonrectangular shape’s bounding box.'
      ],
      checks: [
        {id: 'GP8.1', text: 'Curve grips and Ctrl+Shift segment picks retain their existing precise behavior; open curves do not acquire a generic resize bounding box.'},
        {id: 'GP8.2', text: 'Rectangle/polygon vertex and side controls retain their established priority and edit the intended point or property.'},
        {id: 'GP8.3', text: 'Crop-body pan and crop edge/handle operations keep their established mask semantics and do not become ordinary object move/resize.'},
        {id: 'GP8.4', text: 'Property popups retain their input and established grip priority. Opening/closing a popup does not swallow the next intended board press or mutate the wrong object.'},
        {id: 'GP8.5', text: 'Group movement, portal frame resize and portal-local body input remain usable. A nonrotatable portal has no newly active rotation gesture.'},
        {id: 'GP8.6', text: 'Empty construction-box corners do not select unrelated shape interiors. Each completed edit remains cancellable and has its established undo grouping.'}
      ]
    },
    {
      id: 'GP9', title: 'Keep authored paint, camera behavior and saved output honest', category: 'regression', initialStatus: 'untested',
      concern: 'The selection overlay may adapt on screen; the actual document must still scale, persist and export normally.',
      setup: ['Use the scale-ladder workbook with authored text, a linked image, a rounded/stroked shape and a portal tab. Save a disposable copy and choose an export folder. Record display scale, theme and test zooms.'],
      actions: [
        'Zoom in and out with the wheel over a canvas object and through available camera commands. Inspect the authored content at rest as well as while selected.',
        'Open a menu/scrolling list, hover it and use the wheel; repeat over a portal-local surface that owns wheel input. Return to the canvas and confirm its camera input still works.',
        'Move/resize an object, save the copy, close/reopen it and inspect the committed geometry and wires. Export the HTML artifact and inspect it in a browser.'
      ],
      checks: [
        {id: 'GP9.1', text: 'Authored geometry, text, labels, tabs, image paint and strokes continue to scale with the camera; the compact overlay never enlarges or clamps those authored elements.'},
        {id: 'GP9.2', text: 'Canvas wheel zoom is progressive and remains anchored around the intended pointer area; menu/list and portal-local wheel ownership are respected.'},
        {id: 'GP9.3', text: 'After save/reopen, the committed document has the intended geometry and connection endpoints, with no editing-frame or hit-region state baked into the scene.'},
        {id: 'GP9.4', text: 'Export matches authored content and attachment geometry and contains no compact frames, transient grips or hit-region overlays. Record any native/export visual difference.'}
      ]
    }
  ]
};
