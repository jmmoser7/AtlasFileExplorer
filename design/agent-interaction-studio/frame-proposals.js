/* Frame review images: AI draft concepts and actual JS captures, without native claims. */
(() => {
  'use strict';
  const directory = 'assets/frame-proposals/20261103-v01/';
  const variants = [
    {
      id: 'frame-settings-20261103-v01-closed',
      title: 'Frame Settings · closed dropdowns · AI draft',
      filename: 'frame-settings-v01-closed-draft.png',
      sourceFilename: 'exec-7210860f-4c38-4abe-a597-6b33219a88b7.png',
      promptDescription: 'Frame presentation Settings palette with Video, Webpages and 3D categories; inherited media choices, saved/last webpage view, interactive 3D, and Present last.',
      variant: 'Main concept with dropdowns closed'
    },
    {
      id: 'frame-settings-20261103-v02-start-list',
      title: 'Frame Settings · open Start list · AI draft',
      filename: 'frame-settings-v02-start-list-draft.png',
      sourceFilename: 'exec-760fb000-5863-496e-9486-2a04df2c759b.png',
      promptDescription: 'Companion Frame Settings concept with Video Start expanded to show Media setting, Automatically and On click.',
      variant: 'Companion concept with Video Start list open'
    },
    {
      id: 'frame-settings-20261103-v03-compact',
      title: 'Frame Settings · slim capsules below frame · AI draft',
      directory: 'assets/frame-proposals/20261103-v03/',
      filename: 'frame-settings-v03-compact-draft.png',
      sourceFilename: 'exec-26544a8a-2f90-469c-96d3-3e4be0fcc21a.png',
      promptDescription: 'Revise the closed proposal into three slim capsule rows below the Frame and toolbar, borrowing the fillet palette vocabulary and targeting roughly one quarter of the earlier palette area.',
      variant: 'Compact revision in response to user feedback; unreviewed draft',
      receipt: 'assets/frame-proposals/20261103-v03/receipt.json',
      sourceImage: 'assets/frame-proposals/20261103-v01/frame-settings-v01-closed-draft.png',
      userFeedback: 'more compact coming out the bottom of the frame not over top of it barrow from the fillet menue capsule design. should be able to get everything here inside of 25% the area. slim'
    },
    {
      id: 'frame-settings-20261103-v04-columns',
      title: 'Frame Settings · aligned capsules and wide drawer · AI draft',
      directory: 'assets/frame-proposals/20261103-v04/',
      filename: 'frame-settings-v04-columns-draft.png',
      sourceFilename: 'exec-11e0e081-dcca-40b6-b783-787ba155c34e.png',
      promptDescription: 'Normalize capsule lengths to the longest title in each control column, align both edges, and attach a wide shallow drawer below the blister while retaining the three compact media rows.',
      variant: 'Aligned-column and wide-drawer revision; unreviewed draft',
      receipt: 'assets/frame-proposals/20261103-v04/receipt.json',
      sourceImage: 'assets/frame-proposals/20261103-v03/frame-settings-v03-compact-draft.png',
      userFeedback: 'ok normalize capsule elngthe for longest title in a given column resplct palet slideout asthedic for blister modemake use of the whide aspect ratio avalable'
    },
    {
      "id": "frame-presentations-20261103-v01-add-frames",
      "title": "Create Presentation · Add frames · AI draft",
      "directory": "assets/frame-proposals/20261103-presentation-v01/",
      "filename": "frame-presentation-v01-add-frames-draft.png",
      "sourceFilename": "exec-620046a4-4979-4313-91b9-fb0a1391b352.png",
      "promptDescription": "A Create Presentation drawer with an Add frames toggle, highlighted eligible Frames and circular thumbnails in pick order.",
      "variant": "First Create Presentation concept with Add frames active",
      "receipt": "assets/frame-proposals/20261103-presentation-v01/receipt.json",
      "userFeedback": "order frames will be the new dashboard for create presentation. key one frame can belong to multiple presentations. dashboard should have toggle for add frame-s clicking this will hylight all eligable framesuser can then go around the canvas and select the frames they want auto order by order of selection add circle icon to toolbar for each aditional frame. look to the immage generator toolbar. change the order frames icon to look les dumb start with immage generation"
    },
    {
      "id": "frame-presentations-20261103-v02-compact",
      "title": "Create Presentation · 100-slide strip · AI draft",
      "directory": "assets/frame-proposals/20261103-presentation-v02/",
      "filename": "frame-presentation-v02-compact-draft.png",
      "sourceFilename": "exec-f25e7229-4fe5-41c6-b2e7-84de2bfa3793.png",
      "promptDescription": "A single-row presentation palette with five visible slide thumbnails, navigation chevrons and a slide-number jump control.",
      "variant": "A single-row presentation palette with five visible slide thumbnails, navigation chevrons and a slide-number jump control.",
      "receipt": "assets/frame-proposals/20261103-presentation-v02/receipt.json",
      "sourceImage": "assets/frame-proposals/20261103-presentation-v01/frame-presentation-v01-add-frames-draft.png",
      "userFeedback": "can be more compact also consider what hapens with 50- 100 slides"
    },
    {
      "id": "frame-presentations-20261103-v02-map-hover",
      "title": "Create Presentation · canvas mini-map hover · AI draft",
      "directory": "assets/frame-proposals/20261103-presentation-v02/",
      "filename": "frame-presentation-v02-map-hover-draft.png",
      "sourceFilename": "exec-7c403cd6-7274-4960-9a03-e03b8823ad04.png",
      "promptDescription": "Hovering the presentation name previews the canvas layout of its frames and highlights the current slide.",
      "variant": "Hovering the presentation name previews the canvas layout of its frames and highlights the current slide.",
      "receipt": "assets/frame-proposals/20261103-presentation-v02/receipt.json",
      "sourceImage": "assets/frame-proposals/20261103-presentation-v02/frame-presentation-v02-compact-draft.png",
      "userFeedback": "provide miny map of slide positions on canvas on hover over presentation name"
    },
    {
      "id": "frame-presentations-20261103-v03-reorder",
      "title": "Create Presentation · reorder 100 slides · AI draft",
      "directory": "assets/frame-proposals/20261103-presentation-v03/",
      "filename": "frame-presentation-v03-reorder-draft.png",
      "sourceFilename": "exec-45f058a3-833c-4c78-9034-af84e812b9c8.png",
      "promptDescription": "An expanded presentation reorder tray with a 10 by 10 thumbnail grid, row ranges and a teal insertion marker.",
      "variant": "Expanded Reorder view showing all 100 slides at once",
      "receipt": "assets/frame-proposals/20261103-presentation-v03/receipt.json",
      "sourceImage": "assets/frame-proposals/20261103-presentation-v02/frame-presentation-v02-compact-draft.png",
      "userFeedback": "ok but we will likly need to use the squircle icons to re order slides so all of them need to be visible in a tastful way simaltaniously"
    },
    {
      "id": "frame-presentations-20261103-v04-grid",
      "title": "Create Presentation · direct drag grid · AI draft",
      "directory": "assets/frame-proposals/20261103-presentation-v04/",
      "filename": "frame-presentation-v04-grid-draft.png",
      "sourceFilename": "exec-a054e07b-ed3f-4188-bc66-1cff856276be.png",
      "promptDescription": "Full 100-slide grid with subtle row dividers, multi-selection outlines, a drop marker and Sort on canvas.",
      "variant": "Full 100-slide grid with subtle row dividers, multi-selection outlines, a drop marker and Sort on canvas.",
      "receipt": "assets/frame-proposals/20261103-presentation-v04/receipt.json",
      "sourceImage": "assets/frame-proposals/20261103-presentation-v03/frame-presentation-v03-reorder-draft.png",
      "userFeedback": "no need for the number capsules on the side just leve it as full grid. dropdown should display as select presentation at all times. the name of the presentation will be shown in the dropdown along with th + Reorder is redudndant the user should be able to grip and drag slides around to re order at will. add subtle horizontal deviders between rows. allow for shift and ctrle multy select options allow for drag drop insertions. add option to top bar to sort frames on canvas. automatickly puts them in the same order as the slide show. have a mode for a small number of slides where all slides are in the datum of the top bar"
    },
    {
      "id": "frame-presentations-20261103-v04-inline",
      "title": "Create Presentation · slides in toolbar · AI draft",
      "directory": "assets/frame-proposals/20261103-presentation-v04/",
      "filename": "frame-presentation-v04-inline-draft.png",
      "sourceFilename": "exec-a5c54cfb-56c1-43f8-af6e-945b194b55f7.png",
      "promptDescription": "Five slide previews in the toolbar with the fixed Select presentation label and an open menu showing names and New presentation.",
      "variant": "Five slide previews in the toolbar with the fixed Select presentation label and an open menu showing names and New presentation.",
      "receipt": "assets/frame-proposals/20261103-presentation-v04/receipt.json",
      "sourceImage": "assets/frame-proposals/20261103-presentation-v02/frame-presentation-v02-compact-draft.png",
      "userFeedback": "no need for the number capsules on the side just leve it as full grid. dropdown should display as select presentation at all times. the name of the presentation will be shown in the dropdown along with th + Reorder is redudndant the user should be able to grip and drag slides around to re order at will. add subtle horizontal deviders between rows. allow for shift and ctrle multy select options allow for drag drop insertions. add option to top bar to sort frames on canvas. automatickly puts them in the same order as the slide show. have a mode for a small number of slides where all slides are in the datum of the top bar"
    }
  ];
  const images = (window.StudioPortalData ||= {}).images ||= {};
  const frames = images.frame ||= [];
  for (const variant of variants) {
    if (frames.some(image => image.id === variant.id)) continue;
    frames.push({
      id: variant.id, elementId: 'frame', title: variant.title,
      src: (variant.directory || directory) + variant.filename,
      download: (variant.directory || directory) + variant.filename,
      approval: 'draft-ai-concept', captureKind: 'ai-generated-concept', sourcePhase: '00',
      date: '2026-11-03', dateBasis: 'Generation session date; exact generation time not recorded here.',
      caption: `${variant.variant}. AI-generated visual proposal; not an actual JS screenshot, an approved design, or native/runtime parity evidence.`,
      promptDescription: variant.promptDescription,
      promptDescriptionBasis: variant.receipt ? 'Exact prompt, user feedback and source references are preserved in the linked generation receipt.'
        : 'Description of the requested concept; exact generator prompt is not recorded in this catalog.',
      ...(variant.receipt ? { receipt: variant.receipt, sourceImage: variant.sourceImage, userFeedback: variant.userFeedback } : {}),
      source: { kind: 'image-generation', originalFilename: variant.sourceFilename,
        ...(variant.receipt ? { receipt: variant.receipt, sourceImage: variant.sourceImage } : {}),
        provenance: 'Original generated PNG supplied in this conversation; copied without modification.' }
    });
  }
  const actualCapture = {
  "id": "frame-settings-20261103-v04-actual-js",
  "elementId": "frame",
  "title": "Frame Settings · aligned drawer · actual JS draft",
  "src": "assets/frame-proposals/20261103-v04-js/frame-settings-v04-actual-js-960x900.png",
  "download": "assets/frame-proposals/20261103-v04-js/frame-settings-v04-actual-js-960x900.png",
  "approval": "draft-js-capture-native-unverified",
  "captureKind": "browser-screenshot",
  "sourcePhase": "01",
  "caption": "Actual JavaScript Studio 16:9 sample with the aligned Settings drawer fully open. JS draft; native parity unverified. Earlier AI concepts and baseline remain separate.",
  "sourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
  "jsSourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
  "viewport": {
    "width": 960,
    "height": 900
  },
  "captureViewport": {
    "width": 960,
    "height": 900
  },
  "capture": {
    "url": "http://localhost:8765/?phase=01&tab=frame&sample=1",
    "viewport": {
      "width": 960,
      "height": 900
    }
  },
  "theme": "light",
  "capturedAt": "2026-11-03T17:43:37.739Z",
  "canvasZoom": 0.8,
  "devicePixelRatio": 1,
  "dpi": 96,
  "browser": {
    "name": "Microsoft Edge",
    "version": "156.0.4314.58",
    "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/156.0.0.0 Safari/537.36 Edg/156.0.0.0"
  },
  "fixture": "Isolated sample=1, 16:9 Frame, Settings open; drawer reveal complete.",
  "receipt": "assets/frame-proposals/20261103-v04-js/capture-receipt.json",
  "relevantSourceHashes": {
    "index.html": "888b979c55c8eca6ac70fdbf0f81f0d363fc805f7da0a8cbbe2d86022cd2de12",
    "frame/app.js": "4f617226e182a81b84f40f1a4926c53c050dce4e7873b69e07c0f7a600d4f7a5",
    "frame/render.js": "540d5fabfae4eec452f65fec8a5957379cbf098904731b80e44bb0afcd6cdb12",
    "frame/panels.js": "1f16352550cd81b7cb9959a06768e382e29201e72c8e325a1442ea00482d231b",
    "frame/presentation-settings.js": "ae06cc1bbc1e1c28bee54ef7c5b3c1afc8f2f5a8c5975842ae9a35b4934278ec",
    "frame/presentation-settings.css": "1265b69a4a5403364ea357d94650de454f473ba93e1c91d92692c576a04c9c26"
  },
  "screenshotSHA256": "cc015a5580fa0dd5089f078c5c8be58cab14273a3c94aefb06914a7e915e49b7",
  "referenceImage": "assets/frame-proposals/20261103-v04/frame-settings-v04-columns-draft.png",
  "source": {
    "kind": "browser-screenshot",
    "provenance": "Actual JS capture supplied by implementation owner; original capture and receipt copied without modification.",
    "receipt": "assets/frame-proposals/20261103-v04-js/capture-receipt.json"
  }
};
  if (!frames.some(image => image.id === actualCapture.id)) frames.push(actualCapture);
  const sizeToggleCaptures = [
    {
      "id": "frame-settings-20261103-size-toggle-wide-blister",
      "elementId": "frame",
      "title": "Frame Settings · wide blister · actual JS",
      "src": "assets/frame-proposals/20261103-size-toggle-js/wide-settings-960x900.png",
      "download": "assets/frame-proposals/20261103-size-toggle-js/wide-settings-960x900.png",
      "approval": "draft-js-capture-native-unverified",
      "captureKind": "browser-screenshot",
      "sourcePhase": "01",
      "caption": "Wide fixture with Settings open in the blister drawer. Actual JS draft with native parity unverified.",
      "sourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "jsSourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "viewport": {
        "width": 960,
        "height": 900
      },
      "captureViewport": {
        "width": 960,
        "height": 900
      },
      "capture": {
        "url": "http://localhost:8765/?phase=01&tab=frame&sample=1",
        "viewport": {
          "width": 960,
          "height": 900
        }
      },
      "theme": "light",
      "capturedAt": "2026-11-03T18:21:38.440Z",
      "canvasZoom": 0.8,
      "devicePixelRatio": 1,
      "browser": {
        "version": "156.0.4314.58"
      },
      "fixture": "wide",
      "receipt": "assets/frame-proposals/20261103-size-toggle-js/receipt.json",
      "screenshotSHA256": "9f2e433e9f0023cad5ab9bd1d4eb7121d692de2947f42bc8f2ee0735c8b458eb"
    },
    {
      "id": "frame-settings-20261103-size-toggle-tiny-squircles",
      "elementId": "frame",
      "title": "Frame Settings · tiny squircles · actual JS",
      "src": "assets/frame-proposals/20261103-size-toggle-js/tiny-settings-960x900.png",
      "download": "assets/frame-proposals/20261103-size-toggle-js/tiny-settings-960x900.png",
      "approval": "draft-js-capture-native-unverified",
      "captureKind": "browser-screenshot",
      "sourcePhase": "01",
      "caption": "Tiny fixture with Settings open below the six squircle controls. Actual JS draft with native parity unverified.",
      "sourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "jsSourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "viewport": {
        "width": 960,
        "height": 900
      },
      "captureViewport": {
        "width": 960,
        "height": 900
      },
      "capture": {
        "url": "http://localhost:8765/?phase=01&tab=frame&sample=1",
        "viewport": {
          "width": 960,
          "height": 900
        }
      },
      "theme": "light",
      "capturedAt": "2026-11-03T18:21:38.440Z",
      "canvasZoom": 1,
      "devicePixelRatio": 1,
      "browser": {
        "version": "156.0.4314.58"
      },
      "fixture": "tiny",
      "receipt": "assets/frame-proposals/20261103-size-toggle-js/receipt.json",
      "screenshotSHA256": "00e6d2bd6680efd11549aad0e39c56929e932b34de21c7cfdcd0373f31062868"
    }
  ];
  for (const capture of sizeToggleCaptures) {
    if (!frames.some(image => image.id === capture.id)) frames.push(capture);
  }
  const centeredBlisterCaptures = [
    {
      "id": "frame-settings-20261103-centered-blister-closed-js",
      "elementId": "frame",
      "title": "Frame Settings · centered blister closed · actual JS",
      "src": "assets/frame-proposals/20261103-centered-blister-js/after-closed.png",
      "download": "assets/frame-proposals/20261103-centered-blister-js/after-closed.png",
      "approval": "draft-js-capture-native-unverified",
      "captureKind": "browser-screenshot",
      "sourcePhase": "01",
      "caption": "Actual JavaScript Studio authored fixture with Settings closed. JS draft with native parity unverified.",
      "sourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "jsSourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "viewport": {
        "height": 900,
        "width": 960
      },
      "captureViewport": {
        "height": 900,
        "width": 960
      },
      "capture": {
        "url": "http://localhost:8765/?phase=01&tab=frame&sample=1",
        "viewport": {
          "height": 900,
          "width": 960
        }
      },
      "theme": "light",
      "capturedAt": "2026-11-03T18:46:54.0994350Z",
      "camera": "Fit, 80%",
      "browser": "Microsoft Edge headless, isolated context",
      "fixture": "authored",
      "receipt": "assets/frame-proposals/20261103-centered-blister-js/visual-receipt.json",
      "screenshotSHA256": "5fa9e71afae0163a484fab3674a86f6f90fbfde41713f53776cffb09878381d4"
    },
    {
      "id": "frame-settings-20261103-centered-blister-open-js",
      "elementId": "frame",
      "title": "Frame Settings · centered blister open · actual JS",
      "src": "assets/frame-proposals/20261103-centered-blister-js/after-open.png",
      "download": "assets/frame-proposals/20261103-centered-blister-js/after-open.png",
      "approval": "draft-js-capture-native-unverified",
      "captureKind": "browser-screenshot",
      "sourcePhase": "01",
      "caption": "Actual JavaScript Studio authored fixture with Settings open. JS draft with native parity unverified.",
      "sourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "jsSourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "viewport": {
        "height": 900,
        "width": 960
      },
      "captureViewport": {
        "height": 900,
        "width": 960
      },
      "capture": {
        "url": "http://localhost:8765/?phase=01&tab=frame&sample=1",
        "viewport": {
          "height": 900,
          "width": 960
        }
      },
      "theme": "light",
      "capturedAt": "2026-11-03T18:46:54.0994350Z",
      "camera": "Fit, 80%",
      "browser": "Microsoft Edge headless, isolated context",
      "fixture": "authored",
      "receipt": "assets/frame-proposals/20261103-centered-blister-js/visual-receipt.json",
      "screenshotSHA256": "65d80223c2895d166c77c1d34a04d6cc8a827a3c4a433400494a6044fcbe5a5f"
    }
  ];
  for (const capture of centeredBlisterCaptures) {
    if (!frames.some(image => image.id === capture.id)) frames.push(capture);
  }
  const dimensionCaptures = [
    {
      "id": "frame-dimensions-20261103-blister-spacing-js",
      "elementId": "frame",
      "title": "Frame dimensions · blister spacing · actual JS",
      "src": "assets/frame-proposals/20261103-stringer-spacing-js/wide-stringers.png",
      "download": "assets/frame-proposals/20261103-stringer-spacing-js/wide-stringers.png",
      "approval": "draft-js-capture-native-unverified",
      "captureKind": "browser-screenshot",
      "sourcePhase": "01",
      "caption": "Actual JavaScript Studio wide fixture showing dimension spacing in blister mode. Native parity remains unverified.",
      "sourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "jsSourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "viewport": {
        "width": 960,
        "height": 900
      },
      "captureViewport": {
        "width": 960,
        "height": 900
      },
      "capture": {
        "url": "http://localhost:8765/?phase=01&tab=frame&sample=1",
        "viewport": {
          "width": 960,
          "height": 900
        }
      },
      "theme": "light",
      "capturedAt": "2026-11-03T19:00:19.298Z",
      "camera": {
        "x": 96,
        "y": 173,
        "zoom": 0.8
      },
      "canvasZoom": 0.8,
      "browser": {
        "version": "156.0.4314.58"
      },
      "fixture": "wide",
      "receipt": "assets/frame-proposals/20261103-stringer-spacing-js/receipt.json",
      "screenshotSHA256": "a55ad2723d0506e011e82133575938ce39206d613c8193f2e9b90c3b75765a57"
    },
    {
      "id": "frame-dimensions-20261103-squircle-spacing-js",
      "elementId": "frame",
      "title": "Frame dimensions · squircle spacing · actual JS",
      "src": "assets/frame-proposals/20261103-stringer-spacing-js/rotated-stringers.png",
      "download": "assets/frame-proposals/20261103-stringer-spacing-js/rotated-stringers.png",
      "approval": "draft-js-capture-native-unverified",
      "captureKind": "browser-screenshot",
      "sourcePhase": "01",
      "caption": "Actual JavaScript Studio rotated fixture showing dimension spacing in squircle mode. Native parity remains unverified.",
      "sourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "jsSourceUrl": "http://localhost:8765/?phase=01&tab=frame&sample=1",
      "viewport": {
        "width": 960,
        "height": 900
      },
      "captureViewport": {
        "width": 960,
        "height": 900
      },
      "capture": {
        "url": "http://localhost:8765/?phase=01&tab=frame&sample=1",
        "viewport": {
          "width": 960,
          "height": 900
        }
      },
      "theme": "light",
      "capturedAt": "2026-11-03T19:00:19.298Z",
      "camera": {
        "x": 287.1008979927757,
        "y": 139.3658679906508,
        "zoom": 0.6303892222458312
      },
      "canvasZoom": 0.6303892222458312,
      "browser": {
        "version": "156.0.4314.58"
      },
      "fixture": "rotated",
      "receipt": "assets/frame-proposals/20261103-stringer-spacing-js/receipt.json",
      "screenshotSHA256": "c403f4df986f9b0a8efb3801354b97f914d0ba5e76183f5e8770d99b5ab21bd1"
    }
  ];
  for (const capture of dimensionCaptures) {
    if (!frames.some(image => image.id === capture.id)) frames.push(capture);
  }
  const presentationCaptures = [
  {
    "id": "frame-presentations-20261103-js-presentation-small",
    "elementId": "frame",
    "title": "Create Presentation · Five slides in the toolbar · actual JS",
    "src": "assets/frame-proposals/20261103-presentation-js/five-slides.png",
    "download": "assets/frame-proposals/20261103-presentation-js/five-slides.png",
    "approval": "draft-js-capture-native-unverified",
    "captureKind": "browser-screenshot",
    "sourcePhase": "01",
    "caption": "Five slides in one toolbar, with Add frames highlighting the spare Discussion frame.",
    "file": "five-slides.png",
    "fixture": "presentation-small",
    "sourceUrl": "http://127.0.0.1:8765/?phase=01&tab=frame&sample=1&fixture=presentation-small",
    "viewport": {
      "width": 1280,
      "height": 720
    },
    "deviceScaleFactor": 1,
    "theme": "light",
    "capturedAt": "2026-11-03T19:47:29.223Z",
    "camera": {
      "x": 90,
      "y": 87.95951035781545,
      "zoom": 0.5178907721280602
    },
    "sha256": "178b085945d5e663b24a55648f9441c1cf5712f2661915f9af1d9b21d3638964",
    "receipt": "assets/frame-proposals/20261103-presentation-js/receipt.json",
    "browser": "156.0.4314.58"
  },
  {
    "id": "frame-presentations-20261103-js-presentation-large",
    "elementId": "frame",
    "title": "Create Presentation · 100 slides in one grid · actual JS",
    "src": "assets/frame-proposals/20261103-presentation-js/100-slides.png",
    "download": "assets/frame-proposals/20261103-presentation-js/100-slides.png",
    "approval": "draft-js-capture-native-unverified",
    "captureKind": "browser-screenshot",
    "sourcePhase": "01",
    "caption": "The complete 100-slide grid in a 780 × 720 browser viewport, with three thumbnails selected.",
    "file": "100-slides.png",
    "fixture": "presentation-large",
    "sourceUrl": "http://127.0.0.1:8765/?phase=01&tab=frame&sample=1&fixture=presentation-large",
    "viewport": {
      "width": 780,
      "height": 720
    },
    "deviceScaleFactor": 1,
    "theme": "light",
    "capturedAt": "2026-11-03T19:47:31.069Z",
    "camera": {
      "x": 90,
      "y": -78.41242937853107,
      "zoom": 0.2824858757062147
    },
    "sha256": "49423a2e065ce0f69c59a58ed7ce9fb73a10ec48342ab3d4347a66f623778826",
    "receipt": "assets/frame-proposals/20261103-presentation-js/receipt.json",
    "browser": "156.0.4314.58"
  }
];
  for (const capture of presentationCaptures) {
    if (!frames.some(image => image.id === capture.id)) frames.push(capture);
  }
  const attachedPresentationCaptures = [
  {
    "id": "frame-presentations-20261103-attached-presentation-small",
    "elementId": "frame",
    "title": "Create Presentation · attached five-slide palette · actual JS",
    "src": "assets/frame-proposals/20261103-presentation-host-js/five-slides.png",
    "download": "assets/frame-proposals/20261103-presentation-host-js/five-slides.png",
    "approval": "draft-js-capture-native-unverified",
    "captureKind": "browser-screenshot",
    "sourcePhase": "01",
    "caption": "The presentation palette shares its Frame’s width and lower edge.",
    "file": "five-slides.png",
    "fixture": "presentation-small",
    "sourceUrl": "http://127.0.0.1:8765/?phase=01&tab=frame&sample=1&fixture=presentation-small",
    "viewport": {
      "width": 1280,
      "height": 720
    },
    "deviceScaleFactor": 1,
    "theme": "light",
    "capturedAt": "2026-11-03T19:54:45.268Z",
    "camera": {
      "x": 90,
      "y": 87.95951035781545,
      "zoom": 0.5178907721280602
    },
    "sha256": "5f3704f50106ade395ccc578bb46de66bf82d0faf07ccd26d683dca446b48947",
    "receipt": "assets/frame-proposals/20261103-presentation-host-js/receipt.json",
    "browser": "156.0.4314.58"
  },
  {
    "id": "frame-presentations-20261103-attached-presentation-large",
    "elementId": "frame",
    "title": "Create Presentation · attached 100-slide grid · actual JS",
    "src": "assets/frame-proposals/20261103-presentation-host-js/100-slides.png",
    "download": "assets/frame-proposals/20261103-presentation-host-js/100-slides.png",
    "approval": "draft-js-capture-native-unverified",
    "captureKind": "browser-screenshot",
    "sourcePhase": "01",
    "caption": "All 100 thumbnails are visible with the grid attached to its host Frame.",
    "file": "100-slides.png",
    "fixture": "presentation-large",
    "sourceUrl": "http://127.0.0.1:8765/?phase=01&tab=frame&sample=1&fixture=presentation-large",
    "viewport": {
      "width": 780,
      "height": 720
    },
    "deviceScaleFactor": 1,
    "theme": "light",
    "capturedAt": "2026-11-03T19:54:47.722Z",
    "camera": {
      "x": 134,
      "y": -215.60000000000002,
      "zoom": 0.8
    },
    "sha256": "d78687de02e0e4feba619a300a1e0b142ac20c01c75b54a7c9d56bd4cf97fb86",
    "receipt": "assets/frame-proposals/20261103-presentation-host-js/receipt.json",
    "browser": "156.0.4314.58"
  }
];
  for (const capture of attachedPresentationCaptures) {
    if (!frames.some(image => image.id === capture.id)) frames.push(capture);
  }
  const membershipCapture = {
  "id": "frame-presentations-20261103-membership-js",
  "elementId": "frame",
  "title": "Create Presentation · add and remove frames · actual JS",
  "src": "assets/frame-proposals/20261103-presentation-membership-js/five-slides.png",
  "download": "assets/frame-proposals/20261103-presentation-membership-js/five-slides.png",
  "approval": "draft-js-capture-native-unverified",
  "captureKind": "browser-screenshot",
  "sourcePhase": "01",
  "caption": "Add / remove frames shows a plus on available Frames and a minus on slides already in the presentation.",
  "receipt": "assets/frame-proposals/20261103-presentation-membership-js/receipt.json",
  "browser": "156.0.4314.58",
  "file": "five-slides.png",
  "fixture": "presentation-small",
  "sourceUrl": "http://127.0.0.1:8765/?phase=01&tab=frame&sample=1&fixture=presentation-small",
  "viewport": {
    "width": 1280,
    "height": 720
  },
  "deviceScaleFactor": 1,
  "theme": "light",
  "capturedAt": "2026-11-03T20:11:09.494Z",
  "camera": {
    "x": 90,
    "y": 87.95951035781545,
    "zoom": 0.5178907721280602
  },
  "sha256": "6d3d6aa1c121235e9927b1aa81fdbad6bc4197f0415b8959fa4d38c9dbac8a55"
};
  if (!frames.some(image => image.id === membershipCapture.id)) frames.push(membershipCapture);
  const joinedPresentationCaptures = [
  {
    "id": "frame-presentations-20261103-joined-presentation-small",
    "elementId": "frame",
    "title": "Create Presentation · five-slide joined palette · actual JS",
    "src": "assets/frame-proposals/20261103-presentation-joined-js/five-slides-joined.png",
    "download": "assets/frame-proposals/20261103-presentation-joined-js/five-slides-joined.png",
    "approval": "draft-js-capture-native-unverified",
    "captureKind": "browser-screenshot",
    "sourcePhase": "01",
    "caption": "The frame highlight continues into the attached palette, with a heavier selector chevron.",
    "receipt": "assets/frame-proposals/20261103-presentation-joined-js/receipt.json",
    "browser": "156.0.4314.58",
    "file": "five-slides-joined.png",
    "fixture": "presentation-small",
    "sourceUrl": "http://127.0.0.1:8765/?phase=01&tab=frame&sample=1&fixture=presentation-small",
    "viewport": {
      "width": 1280,
      "height": 720
    },
    "deviceScaleFactor": 1,
    "theme": "light",
    "capturedAt": "2026-11-03T20:29:11.616Z",
    "camera": {
      "x": 90,
      "y": 87.95951035781545,
      "zoom": 0.5178907721280602
    },
    "sha256": "ce9b3155fd9ccb1c74f809f9bbdc4cb5c41963fb8d8c930b22ddb801f5f65423"
  },
  {
    "id": "frame-presentations-20261103-joined-presentation-large",
    "elementId": "frame",
    "title": "Create Presentation · 100-slide joined grid · actual JS",
    "src": "assets/frame-proposals/20261103-presentation-joined-js/100-slides-joined.png",
    "download": "assets/frame-proposals/20261103-presentation-joined-js/100-slides-joined.png",
    "approval": "draft-js-capture-native-unverified",
    "captureKind": "browser-screenshot",
    "sourcePhase": "01",
    "caption": "All 100 thumbnails fit in ten rows at 780 by 720; three are selected for cross-row drag.",
    "receipt": "assets/frame-proposals/20261103-presentation-joined-js/receipt.json",
    "browser": "156.0.4314.58",
    "file": "100-slides-joined.png",
    "fixture": "presentation-large",
    "sourceUrl": "http://127.0.0.1:8765/?phase=01&tab=frame&sample=1&fixture=presentation-large",
    "viewport": {
      "width": 780,
      "height": 720
    },
    "deviceScaleFactor": 1,
    "theme": "light",
    "capturedAt": "2026-11-03T20:29:14.835Z",
    "camera": {
      "x": 134,
      "y": -215.60000000000002,
      "zoom": 0.8
    },
    "sha256": "dda67ba013c2daec686e174f5039ba5b212792ee581b6e928cf7606a4a78156c"
  }
];
  for (const capture of joinedPresentationCaptures) {
    if (!frames.some(image => image.id === capture.id)) frames.push(capture);
  }
  const phase03FrameCaptures = [
  {
    "id": "frame-phase03-frame-five-native",
    "elementId": "frame",
    "title": "Create Presentation · five-slide palette · Phase 03 Rust",
    "src": "assets/frame-proposals/20261103-phase03-native/frame-five-native.png",
    "download": "assets/frame-proposals/20261103-phase03-native/frame-five-native.png",
    "approval": "draft-native-canvas-specimen",
    "captureKind": "native-screenshot",
    "sourcePhase": "03",
    "caption": "The small Rust specimen uses an attached palette with five inline slide thumbnails.",
    "renderer": "egui software raster",
    "fixture": "presentation-small",
    "capturedAt": "2026-11-03T21:23:44.6652087Z",
    "imageSize": {
      "width": 1280,
      "height": 720
    },
    "zoom": 0.51789075,
    "theme": "light",
    "sha256": "0e7d5699ca82edd235301dd73c2be8366709822e50d7251f4f5e3c7044e1134a",
    "receipt": "assets/frame-proposals/20261103-phase03-native/receipt.json"
  },
  {
    "id": "frame-phase03-frame-squircle-native",
    "elementId": "frame",
    "title": "Create Presentation · small Frame squircle palette · Phase 03 Rust",
    "src": "assets/frame-proposals/20261103-phase03-native/frame-squircle-native.png",
    "download": "assets/frame-proposals/20261103-phase03-native/frame-squircle-native.png",
    "approval": "draft-native-canvas-specimen",
    "captureKind": "native-screenshot",
    "sourcePhase": "03",
    "caption": "A smaller Frame keeps its external squircle strip and a separate palette.",
    "renderer": "egui software raster",
    "fixture": "presentation-small",
    "capturedAt": "2026-11-03T21:23:44.8758567Z",
    "imageSize": {
      "width": 1280,
      "height": 720
    },
    "zoom": 0.51789075,
    "theme": "light",
    "sha256": "da8e94c6b6ba9ea1b72c20b32898e25d3f0a502a56e09b6d452a4f04a9b89ff4",
    "receipt": "assets/frame-proposals/20261103-phase03-native/receipt.json"
  },
  {
    "id": "frame-phase03-frame-100-live",
    "elementId": "frame",
    "title": "Create Presentation · 100-slide native window · Phase 03 Rust",
    "src": "assets/frame-proposals/20261103-phase03-native/frame-100-live.jpg",
    "download": "assets/frame-proposals/20261103-phase03-native/frame-100-live.jpg",
    "approval": "draft-native-canvas-specimen",
    "captureKind": "native-screenshot",
    "sourcePhase": "03",
    "caption": "All 100 thumbnails are visible in the launched native canvas.",
    "renderer": "Windows live window capture",
    "fixture": "presentation-large",
    "capturedAt": "2026-11-03T21:25:20.2881084Z",
    "imageSize": {
      "width": 782,
      "height": 750
    },
    "zoom": 0.8,
    "theme": "light",
    "sha256": "a3d668ae430b067f137091c92946611c7abb17ce7901f6998b3cef062dac23a5",
    "receipt": "assets/frame-proposals/20261103-phase03-native/receipt.json"
  }
];
  for (const capture of phase03FrameCaptures) {
    if (!frames.some(image => image.id === capture.id)) frames.push(capture);
  }
  window.StudioPortalData.nativeCanvas = {
  "frame": {
    "capture": "assets/frame-proposals/20261103-phase03-native/frame-100-live.jpg",
    "report": "contract-viewer.html?document=contracts/frame-phase-03-receipt.md"
  }
};
  const phase04FrameCaptures = [
  {
    "id": "frame-phase04-slate-five",
    "elementId": "frame",
    "title": "Create Presentation · five slides · actual Slate",
    "src": "assets/frame-proposals/20261103-phase04-slate/slate-five-live.jpg",
    "download": "assets/frame-proposals/20261103-phase04-slate/slate-five-live.jpg",
    "approval": "integrated-review-candidate",
    "captureKind": "native-screenshot",
    "sourcePhase": "04",
    "caption": "The first integration build shows five inline slide thumbnails in the attached palette.",
    "renderer": "Windows live window capture",
    "fixture": "presentation-small",
    "capturedAt": "2026-11-03T22:21:15.8396260Z",
    "imageSize": {
      "width": 1440,
      "height": 900
    },
    "zoom": 0.8,
    "theme": "light",
    "sha256": "7c459be32af6339d748362b7bbaa7af55659642feb242868a40c8503b89a7a7f",
    "binarySha256": "0c389a1de689a2299340c06b5d9fb6d9c9f153d35c6bd1aac102d3fedd910deb",
    "receipt": "assets/frame-proposals/20261103-phase04-slate/receipt.json"
  },
  {
    "id": "frame-phase04-slate-100",
    "elementId": "frame",
    "title": "Create Presentation · 100 slides · actual Slate",
    "src": "assets/frame-proposals/20261103-phase04-slate/slate-100-live.jpg",
    "download": "assets/frame-proposals/20261103-phase04-slate/slate-100-live.jpg",
    "approval": "integrated-review-candidate",
    "captureKind": "native-screenshot",
    "sourcePhase": "04",
    "caption": "The final review build shows all 100 slides in the attached grid. Cross-row drag was verified and undone.",
    "renderer": "Windows live window capture",
    "fixture": "presentation-large",
    "capturedAt": "2026-11-03T22:45:50.4621767Z",
    "imageSize": {
      "width": 1620,
      "height": 1032
    },
    "zoom": 0.8,
    "theme": "light",
    "sha256": "2029364dd6def699805bd0dedb7dbc73046df10d2275472e63a8e61b3000ed15",
    "binarySha256": "a60ab6883804ec47e41e789d4e3b760d02767c93301747f4f8b5fbf7e4d86aff",
    "receipt": "assets/frame-proposals/20261103-phase04-slate/receipt.json"
  }
];
  for (const capture of phase04FrameCaptures) {
    if (!frames.some(image => image.id === capture.id)) frames.push(capture);
  }
  const frameFollowup = {
    id: "frame-phase04-blister-selection-fix",
    elementId: "frame",
    title: "Frame selection · blister outline · actual Slate",
    src: "assets/frame-proposals/20261103-phase04-frame-fixes/blister-native.jpg",
    download: "assets/frame-proposals/20261103-phase04-frame-fixes/blister-native.jpg",
    receipt: "assets/frame-proposals/20261103-phase04-frame-fixes/receipt.json",
    approval: "integrated-review-candidate",
    captureKind: "native-screenshot",
    sourcePhase: "04",
    caption: "The selection rail bends around the toolbar while the dimension stringer remains below it.",
    renderer: "Windows live window capture",
    fixture: "presentation-large",
    capturedAt: "2026-11-03T23:59:40.8085262Z",
    imageSize: { width: 1620, height: 1032 },
    zoom: 0.8,
    theme: "light",
    sha256: "eed2ff7fbff569514f5be05d4177a56b92dac982a705ed706bd722a9d8b68ce8",
    binarySha256: "a01ff9d1307dcda884b91368dd36bd20c24ad9eabcd484c48793e3144ced0897"
  };
  if (!frames.some(image => image.id === frameFollowup.id)) frames.push(frameFollowup);
  window.StudioPortalData.summaries['04'].frame = {
  "note": "Native Frame presentation palette and Board wheel zoom. The updated Slate debug build is running with the 100-slide fixture.",
  "entries": [
    {
      "label": "First Slate build",
      "seconds": 965,
      "detail": "Five and 100 slides displayed. Cross-row drag did not persist."
    },
    {
      "label": "Slate rebuild after pointer fix",
      "seconds": 805,
      "detail": "Slide 47 moved to slot 11 and saved. Undo restored all 100 positions."
    },
    {
      "label": "Final review launch",
      "seconds": null,
      "detail": "Launched at 22:38:47 UTC. Live checks covered five- and 100-slide layouts, membership, drag and undo, Settings dismissal, playback order and wheel zoom."
    }
  ],
  "records": [
    {
      "id": "frame-native-first",
      "label": "First Slate build",
      "category": "Build",
      "status": "Built; drag check failed",
      "start": "2026-11-03T22:03:18.637Z",
      "end": "2026-11-03T22:19:23.6633228Z",
      "seconds": 965,
      "evidence": "http://127.0.0.1:8765/contract-viewer.html?document=contracts/frame-phase-04-receipt.md",
      "result": "Five and 100 slides displayed. Cross-row drag did not persist."
    },
    {
      "id": "frame-native-rebuild",
      "label": "Slate rebuild after pointer fix",
      "category": "Build",
      "status": "Built and launched",
      "start": "2026-11-03T22:25:09.565Z",
      "end": "2026-11-03T22:38:35.1966785Z",
      "seconds": 805,
      "evidence": "http://127.0.0.1:8765/contract-viewer.html?document=contracts/frame-phase-04-receipt.md",
      "result": "Slide 47 moved to slot 11 and saved. Undo restored all 100 positions."
    }
  ],
  "unknownDurations": {
    "writing": null,
    "debugging": null,
    "interaction testing": null
  }
};
  const frameFollowupBuilds = [
  {
    "id": "frame-followup-first",
    "label": "Frame interaction build",
    "category": "Build",
    "status": "Built and tested",
    "start": "2026-11-03T23:37:54.2949588Z",
    "end": "2026-11-03T23:49:58.0952797Z",
    "seconds": 722,
    "evidence": "http://127.0.0.1:8765/contract-viewer.html?document=contracts/frame-phase-04-receipt.md",
    "result": "Verified grip, resize, blister, thumbnail wheel and live presentation playback."
  },
  {
    "id": "frame-followup-superseded",
    "label": "Decoder build attempt",
    "category": "Build",
    "status": "Superseded",
    "start": "2026-11-03T23:51:29.1792437Z",
    "end": "2026-11-03T23:53:18.5461637Z",
    "seconds": 109,
    "evidence": "http://127.0.0.1:8765/contract-viewer.html?document=contracts/frame-phase-04-receipt.md",
    "result": "Stopped before final link when the live 3D drag check exposed a lost gesture."
  },
  {
    "id": "frame-followup-orbit",
    "label": "Decoder and orbit build",
    "category": "Build",
    "status": "Built and tested",
    "start": "2026-11-03T23:54:47.8835793Z",
    "end": "2026-11-04T00:18:51.8160392Z",
    "seconds": 1443,
    "evidence": "http://127.0.0.1:8765/contract-viewer.html?document=contracts/frame-phase-04-receipt.md",
    "result": "Verified live fast orbit and repeat video playback."
  },
  {
    "id": "frame-followup-settings",
    "label": "Final Settings build",
    "category": "Build",
    "status": "Built and launched",
    "start": "2026-11-04T00:05:33.1987342Z",
    "end": "2026-11-04T00:29:30.2614493Z",
    "seconds": 1436,
    "evidence": "http://127.0.0.1:8765/contract-viewer.html?document=contracts/frame-phase-04-receipt.md",
    "result": "Includes about 13 min 18 s waiting for the preceding build. Verified the dropdown choice and live Last view."
  }
];
  const frameTimeReport = window.StudioPortalData.summaries['04'].frame;
  frameTimeReport.note = "Native Frame presentation integration and the follow-up grip, zoom, resize and media fixes. The final Slate build is open on the five-slide real-media review workbook.";
  frameTimeReport.records.push(...frameFollowupBuilds);
  frameTimeReport.entries.push(...frameFollowupBuilds.map(b => ({
    label: b.label, seconds: b.seconds, detail: b.result
  })));
})();

(() => {
  const frames = (window.StudioPortalData.images.frame ||= []);
  const capture = {
  "id": "frame-launcher-20261104-browser",
  "title": "Rust Studio launcher \u00b7 actual dashboard",
  "src": "assets/frame-proposals/20261104-frame-selection/studio-launcher.png",
  "caption": "Actual dashboard capture after Launch Rust Studio opened the standalone native Frame canvas. The launcher was verified and the test window was closed. Native selection review is pending separately.",
  "sourceUrl": "http://127.0.0.1:8765/",
  "captureViewport": {
    "width": 1280,
    "height": 720
  },
  "receipt": "assets/frame-proposals/20261104-frame-selection/launcher-receipt.json",
  "approval": "review-capture",
  "sha256": "5415a0d3f3963192d86b53b6b82401630a1b279213a3d01c7a7bdcd43e326b03"
};
  if (!frames.some(image => image.id === capture.id)) frames.push(capture);
})();

(() => {
  const report = window.StudioPortalData.summaries['04'].frame;
  const records = [
  {
    "id": "frame-selection-source-check",
    "label": "Frame selection source check",
    "category": "Check",
    "status": "Passed before final changes",
    "start": null,
    "end": null,
    "seconds": 264,
    "source": "target/frame-followup/frame-selection-check.log",
    "result": "Source check passed with nine warnings. Predates final child guards, slide cuts and method extractions."
  },
  {
    "id": "frame-selection-align-regressions",
    "label": "Frame selection alignment regressions",
    "category": "Test",
    "status": "Passed before final changes",
    "start": "2026-11-04T13:09:11.9926420Z",
    "end": "2026-11-04T13:37:41.7258832Z",
    "seconds": 1709.733241,
    "compilerSeconds": 1698,
    "source": "target/frame-followup/frame-selection-tests.log",
    "result": "23 passed; 0 failed; 1 intentionally ignored visual capture; 2512 filtered. Test execution 8.04 s. Predates final child guards, slide cuts and method extractions."
  }
];
  report.note = 'Native Frame presentation integration and selection follow-up. Completed checks are recorded below. Final Slate build and actual grouping/instant-transition review are pending.';
  for (const record of records) {
    if (report.records.some(previous => previous.id === record.id)) continue;
    report.records.push(record);
    report.entries.push({label: record.label, seconds: record.seconds, detail: record.result});
  }
})();

(() => {
  const report = window.StudioPortalData.summaries['04'].frame;
  const records = [
  {
    "id": "frame-selection-build-superseded",
    "label": "Frame selection build attempt",
    "category": "Build",
    "status": "Superseded",
    "start": "2026-11-04T13:38:42.1283178Z",
    "end": "2026-11-04T13:40:52.2642388Z",
    "seconds": 130.1404016,
    "source": "target/frame-selection/build-superseded.log",
    "result": "Deliberately stopped for the late present.rs file-size miss; unchanged helper extracted before retry."
  },
  {
    "id": "frame-selection-build-reviewed-first",
    "label": "Frame selection first native build",
    "category": "Build",
    "status": "Built; live sweep failed",
    "start": "2026-11-04T13:41:59.6842779Z",
    "end": "2026-11-04T13:51:16.1390767Z",
    "seconds": 556.4606132,
    "source": "target/frame-selection/build-reviewed-first.log",
    "result": "Hosted-body drag, carried alignment/Undo, child release and direct slide cuts passed. Blank-canvas sweeps and Ctrl+A normalization failed, requiring an input fix."
  },
  {
    "id": "frame-selection-final-input-check",
    "label": "Frame selection input-fix type check",
    "category": "Check",
    "status": "Passed; runtime pending",
    "start": "2026-11-04T14:08:50.0917621Z",
    "end": "2026-11-04T14:10:09.3525319Z",
    "seconds": 79.2648672,
    "source": "target/frame-selection/final-input-check.log",
    "result": "cargo check --locked -p slate --tests --jobs 2 passed. New raw-pointer regression compiled but was not executed. Final native sweep review pending."
  }
];
  report.note = 'Native Frame selection follow-up. The first completed build passed drag, alignment, Undo, child release and direct slide cuts, but failed live blank-canvas sweeps. Input fixes passed a metadata type check. Final rebuild and actual sweep review are pending.';
  for (const record of records) {
    if (report.records.some(previous => previous.id === record.id)) continue;
    report.records.push(record);
    report.entries.push({label: record.label, seconds: record.seconds, detail: record.result});
  }
})();

(() => {
  const frames = window.StudioPortalData.images.frame;
  const capture = {
  "id": "frame-selection-20261104-native-final",
  "title": "Frame ownership selection \u00b7 actual Slate",
  "src": "assets/frame-proposals/20261104-frame-selection/native-selected-frames.png",
  "caption": "Actual Slate window with three selected Frame roots. Global Window sweep, hosted-body drag, carried Align Top, Undo, Ctrl+A roots, child release and instant slide change passed on the disposable review workbook. Raw-pointer regression typechecked only.",
  "sha256": "4d8b7d73e86fbd0d62f57a16616cb4acdf8c4ff08a993edb3b60e0d545096d29",
  "captureViewport": {
    "width": 1440,
    "height": 900
  },
  "receipt": "assets/frame-proposals/20261104-frame-selection/native-final-receipt.json",
  "approval": "review-capture",
  "source": "Production Slate executable 2200cc4495aa96eec7892670928072eaf9bdbd17724a39c6a2fe5a7cf90a8c97"
};
  if (!frames.some(image => image.id === capture.id)) frames.push(capture);
  const report = window.StudioPortalData.summaries['04'].frame;
  const record = {
  "id": "frame-selection-build-final",
  "label": "Frame selection final native build",
  "category": "Build",
  "status": "Built; targeted native review passed",
  "start": "2026-11-04T14:11:14.6007670Z",
  "end": "2026-11-04T14:19:37.9979516Z",
  "seconds": 503.400617,
  "evidence": "http://127.0.0.1:8765/contract-viewer.html?document=contracts/frame-phase-04-receipt.md",
  "source": "target/frame-selection/build.log",
  "result": "Actual global Window sweep, Frame-root Ctrl+A, nine-node hosted drag, carried Align Top/Undo, child release and instant slide cut passed. Disposable workbook. New raw-pointer regression typechecked only."
};
  report.note = 'Native Frame selection follow-up completed its targeted production review. Final build and actual global selection, hosted drag, alignment/Undo, child release and instant slide cut passed. The earlier failed sweep and superseded attempt remain recorded. The app is open on a disposable three-Frame workbook; the new raw-pointer regression was typechecked only.';
  if (!report.records.some(previous => previous.id === record.id)) {
    report.records.push(record);
    report.entries.push({label: record.label, seconds: record.seconds, detail: record.result});
  }
})();
