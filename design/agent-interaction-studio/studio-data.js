/* Static historical evidence. Missing element keys mean no recorded data.
   Phase attribution is retrospective; this file starts no builds or services. */
(() => {
  'use strict';
  const source = 'Recorded v1.0.5 release audit; all timestamps UTC.';
  const run = 'https://github.com/jmmoser7/AtlasFileExplorer/actions/runs/';
  const release = 'https://github.com/jmmoser7/AtlasFileExplorer/releases/tag/v1.0.5';
  const time = value => `2026-11-03T${value}Z`;
  const elapsed = (start, end) => (Date.parse(end) - Date.parse(start)) / 1000;
  const record = (id, label, category, status, start, end, detail, extra = {}) => ({
    id, label, category, status, start: time(start), end: time(end),
    seconds: elapsed(time(start), time(end)), source, detail, ...extra
  });
  const image = (version, title, approval, caption, promptDescription) => ({
    id: `workflow-${version}`, title, approval, caption, promptDescription,
    src: `assets/studio-workflow/studio-workflow-v${version}-20261103.png`,
    download: `assets/studio-workflow/studio-workflow-v${version}-20261103.png`,
    date: '2026-11-03', dateBasis: 'Known session date; exact generation time not asserted.',
    promptDescriptionBasis: 'Iteration-request description, not a recovered verbatim prompt.'
  });
  const note = 'Retrospective global v1.0.5 audit, not a per-tool measurement or live build interface. '
    + 'Elapsed windows include waiting, builds and repairs; writing and debugging durations are unknown. '
    + 'Nested and parallel records overlap and must not be added to elapsed totals.';
  const localBuild = record('local-test-build-4172134d', 'Local test executable build (older source)',
    'build', 'completed', '01:07:47.771', '01:31:52.409',
    'Inside the integration window; source 4172134d predates the final candidate. No current native parity claim.',
    { overlap: true, sourceCommit: '4172134d', recordedSeconds: 1444.637,
      precisionNote: 'Endpoint subtraction yields 1444.638s; historical command receipt reported 1444.637s. Round human presentation.' });
  const records = [
    record('windows-first-125688304198', 'First Windows job', 'validation', 'failed',
      '02:27:26', '03:02:14',
      'Retained failed full-job evidence. An isolated diagnostic pass did not turn this gate green.',
      { evidence: `${run}41566142706/job/125688304198` }),
    record('windows-first-workspace', 'First workspace tests', 'test', 'failed',
      '02:29:03', '03:01:53',
      'Slate 2450 passed/0 failed/89 ignored; Atlas 65 passed/1 failed/1 ignored (shell 0x7c). Diagnostic case later passed.',
      { overlap: true, parentId: 'windows-first-125688304198', compilerSeconds: 1553,
        result: { slate: { passed: 2450, failed: 0, ignored: 89 }, atlas: { passed: 65, failed: 1, ignored: 1 } },
        evidence: `${run}41566142706/job/125688304198` }),
    record('first-failure-triage', 'Failure triage window', 'triage', 'recorded',
      '03:02:14', '03:04:24',
      'Coarse elapsed window; cause was audited. This is not a measured debugging or writing duration.'),
    record('windows-retry-125700321628', 'Unchanged Windows retry', 'validation', 'passed',
      '03:04:31', '03:52:23', 'Unchanged full-job retry passed; the preceding failed run remains in the record.',
      { evidence: `${run}41566142706/job/125700321628` }),
    record('windows-retry-workspace', 'Retry workspace tests', 'test', 'passed',
      '03:06:20', '03:37:34', 'Atlas 66 passed/0 failed/1 ignored; Slate 2450 passed/0 failed/89 ignored.',
      { overlap: true, parentId: 'windows-retry-125700321628', compilerSeconds: 1478,
        result: { atlas: { passed: 66, failed: 0, ignored: 1 }, slate: { passed: 2450, failed: 0, ignored: 89 } },
        evidence: `${run}41566142706/job/125700321628` }),
    record('windows-retry-package', 'Retry package step', 'package', 'passed',
      '03:37:34', '03:52:07', 'Includes release compilation and packaging, not compilation alone.',
      { overlap: true, parentId: 'windows-retry-125700321628', compilerSeconds: 812,
        evidence: `${run}41566142706/job/125700321628` }),
    record('stable-windows-125716003617', 'Stable tag Windows job', 'validation', 'passed',
      '03:53:54', '04:53:21', 'Required stable build; separate from preview builds.',
      { evidence: `${run}41574895968/job/125716003617` }),
    record('stable-install-upgrade', 'Stable real install/upgrade check', 'install-upgrade', 'passed',
      '03:54:54', '03:58:07', 'Real installation/upgrade verification.',
      { overlap: true, parentId: 'stable-windows-125716003617',
        evidence: `${run}41574895968/job/125716003617` }),
    record('stable-workspace', 'Stable workspace tests', 'test', 'passed',
      '03:58:07', '04:35:26', 'Includes compilation and workspace test execution.',
      { overlap: true, parentId: 'stable-windows-125716003617', compilerSeconds: 1825,
        evidence: `${run}41574895968/job/125716003617` }),
    record('stable-package', 'Stable package step', 'package', 'passed',
      '04:35:27', '04:51:26', 'Includes release compilation, packaging and verification.',
      { overlap: true, parentId: 'stable-windows-125716003617', compilerSeconds: 894,
        evidence: `${run}41574895968/job/125716003617` }),
    record('stable-publish-125734570328', 'Publish job', 'publish', 'passed',
      '04:53:24', '04:54:02', 'Public release appeared at 04:53:59Z.',
      { evidence: `${run}41574895968/job/125734570328` }),
    record('final-public-verification', 'Final downloadable release verification', 'verification', 'passed',
      '04:54:02', '04:56:23.842', 'Metadata, feed/package digests and payload verified; binaries not executed.',
      { evidence: release }),
    record('main-preview-41574882622', 'Main preview workflow (parallel)', 'validation', 'recorded',
      '03:53:43', '04:40:44', 'Overlapped the stable path; do not add to elapsed delivery time.',
      { overlap: true, parallel: true, evidence: `${run}41574882622` }),
    { id: 'main-ci-41574882543', label: 'Main CI (parallel)', category: 'validation', status: 'overlapped',
      start: null, end: null, seconds: null, source,
      detail: 'Overlapped the release; exact interval is not supplied in this audit.',
      overlap: true, parallel: true, evidence: `${run}41574882543` }
  ];
  const start = time('00:15:09.141');
  const boundary = time('02:26:36');
  const end = time('04:56:23.842');
  const integration = {
    title: 'v1.0.5 historical integration/repair window', start, end: boundary,
    seconds: elapsed(start, boundary), note, records: [localBuild],
    unknownDurations: { writing: null, debugging: null }, download: 'studio-data.js',
    entries: [
      { label: 'Integration/repair elapsed window', seconds: elapsed(start, boundary),
        detail: 'First release request to final candidate workflow start; retrospective attribution, not typing time.' },
      { label: 'Local test build (overlapping, older source)', seconds: localBuild.seconds,
        detail: 'Included inside this window; source 4172134d was older than final. Do not add to elapsed.' }
    ]
  };
  const releaseSummary = {
    title: 'v1.0.5 historical validation/publication window', start: boundary, end,
    seconds: elapsed(boundary, end), note, records,
    unknownDurations: { writing: null, debugging: null }, download: 'studio-data.js',
    entries: [{ label: 'Validation/publication elapsed window', seconds: elapsed(boundary, end),
      detail: 'Final candidate workflow start to completed public verification.' },
    ...records.filter(item => [
      'windows-first-125688304198', 'windows-retry-125700321628', 'stable-windows-125716003617',
      'stable-publish-125734570328', 'final-public-verification', 'main-preview-41574882622'
    ].includes(item.id)).map(item => ({ label: item.label, seconds: item.seconds, detail: item.detail }))]
  };
  window.StudioPortalData = {
    version: 1,
    images: {
      video: [{
        id:'video-native-early-20261104',title:'Early native Video Studio · real test-pattern clip',
        src:'assets/video-studio/20261104-early/video-native-early.jpg',
        date:'2026-11-04',approval:'review-capture',
        caption:'Early in-process layout/software slice; recovery/stress not yet proved in this binary. The color-bar test pattern is the real video-motion.mp4 fixture reference, not a stripe defect. An immediate AX snapshot initially suggested Pause failure; that inference was not conclusive because AX can lag one frame. Settled V3 screenshot plus subsequent AX later confirmed Paused at 0.625.',
        initialObservation:'Immediate early AX snapshot was interpreted as Pause failed. Retained as an initial observation, not proof of a production Pause defect.',
        clarification:'V3 component UI subsequently passed settled Play/Pause, verified helper crash recovery and original-clip retry. Replay-opening and adaptive corpus tests remained blocked at that V3 evidence stage.',
        promptDescription:'Native capture of the early standalone canvas using production Media Foundation decode; no image generation or paid inference.',
        source:'Early video_canvas.exe SHA256 4da9c447c42ab9b54ffce5aff57bdc07fe087a119dc38826dcf62e519d5ebf18',
        receipt:'assets/video-studio/20261104-early/capture-source-receipt.json'
      },{
        id:'video-native-v3-crash-recovered-20261104',title:'V3 component UI · different clip after verified helper crash',
        src:'assets/video-studio/20261104-v3-ui/decoder-crash-recovered.jpg',date:'2026-11-04',approval:'review-capture',
        caption:'V3 component UI only: after the owned helper PID, path and parent were verified and the helper was intentionally stopped, one click played a different known valid clip with real frame/time advance. Settled Play/Pause also passed. Replay-opening and adaptive corpus tests were still blocked; this is not integration or release approval.',
        promptDescription:'Actual native Video Studio capture after externally crashing its verified decoder child and playing another clip; no image generation.',
        source:'V3 video_canvas.exe SHA256 28770C81C8629F2C91599B2FBA8B0218C38EF17FC8D1161DB4BC92EC0F864CE1',
        receipt:'assets/video-studio/20261104-v3-ui/ui-proof.json',binaryReceipt:'assets/video-studio/20261104-v3-ui/binary-receipt.json',
        crashReceipt:'assets/video-studio/20261104-v3-ui/verified-worker-crash.json'
      },{
        id:'video-native-v3-original-retried-20261104',title:'V3 component UI · original clip retried after crash',
        src:'assets/video-studio/20261104-v3-ui/original-clip-retried.jpg',date:'2026-11-04',approval:'review-capture',
        caption:'V3 component UI only: one click retried the original clip after the verified decoder-child crash; original clip Playing and the other clip Paused were observed. Settled Pause was confirmed at 0.625 with screenshot plus subsequent AX. Replay-opening and adaptive corpus tests were still blocked; this capture proves only the exercised component UI cases.',
        promptDescription:'Actual native Video Studio capture after one-click retry of the original clip following decoder-child crash recovery; no image generation.',
        source:'V3 video_canvas.exe SHA256 28770C81C8629F2C91599B2FBA8B0218C38EF17FC8D1161DB4BC92EC0F864CE1',
        receipt:'assets/video-studio/20261104-v3-ui/ui-proof.json',binaryReceipt:'assets/video-studio/20261104-v3-ui/binary-receipt.json',
        crashReceipt:'assets/video-studio/20261104-v3-ui/verified-worker-crash.json'
      },{
        id:'video-native-v8-clear-reload-20261104',title:'V8 component UI · Clear and ordinary picker reload',
        src:'assets/video-studio/20261104-v8-lifecycle/clear-reload.jpg',date:'2026-11-04',approval:'review-capture',
        caption:'V8 component UI lifecycle only: Clear removed clips, all positions, decoded cache and texture estimate, with no worker. Ordinary Open video file picker reloaded the baseline MP4 and all 96 positions; one click produced 8 real frames then Ended, with no additional failure. This binary predates lossless strip delivery. Historical V9 native adaptive scrub was blocked; V10 later passed the full component stress. This V8 capture is not app-integration or release approval.',
        promptDescription:'Actual native capture after Clear, normal file-picker reload and one-click baseline playback. No image generation.',
        source:'V8 video_canvas.exe SHA256 AF46EFD2488AC6AD6AF8B0535BA4780EC2C466DD117E01B6251A487A75C2EBD6; worker SHA256 73CAD25AEB994CF9FD9A9A531EE8B47EE6545DE1CA9A88DD8BAD0713D733CBC7',
        receipt:'assets/video-studio/20261104-v8-lifecycle/lifecycle-proof.json',binaryReceipt:'assets/video-studio/20261104-v8-lifecycle/binary-receipt.json',
        failureHistory:['assets/video-studio/20261104-v8-lifecycle/v7-stress-first.json','assets/video-studio/20261104-v8-lifecycle/v8-stress-first.json','assets/video-studio/20261104-v8-lifecycle/v9-stress-first.json']
      },{
        id:'video-native-v10-stress-passed-20261104',title:'V10 native component · all 92 stress actions passed',
        src:'assets/video-studio/20261104-v10-pass/native-stress-passed.jpg',date:'2026-11-04',approval:'review-capture',
        caption:'V10 component UI: 92/92 observed actions passed in 97.744 seconds with 162 real playback frames across five supported clips including WebM, one expected malformed failure and successful recovery. Peak retained RGBA 41,889,600 / 41,943,040 bytes; peak queue 1,536,000 / 8 MiB. Core25, real12 and gate3 tests passed. Follow-up Play/Pause and native scrub callback also passed. This is component evidence on Iris Plus, not physical Iris Xe, app-integration or release approval.',
        promptDescription:'Actual native Video Studio capture after the event-gated full stress run. Earlier failed runs are retained; no image generation.',
        source:'V10 video_canvas.exe SHA256 1A08245B7DD2AD5ADDD1A288D4B0F119D4802AF219E53A4A8B3C71F2555799BE; worker SHA256 ED13A6EBEE3C749CEAE7B752DA69E8301C433856BF873FA3DB0D4F453A84CD4B',
        receipt:'assets/video-studio/20261104-v10-pass/stress-pass.json',binaryReceipt:'assets/video-studio/20261104-v10-pass/binary-receipt.json',
        sourceReceipt:'assets/video-studio/20261104-v10-pass/source-candidate.json',pointerReceipt:'assets/video-studio/20261104-v10-pass/native-ui-proof.json'
      },{
        id:'video-native-v10-dashboard-launch-20261104',title:'V10 native entry · launched from the visible Studio button',
        src:'assets/video-studio/20261104-v10-pass/dashboard-launch.jpg',date:'2026-11-04',approval:'review-capture',
        caption:'V10 standalone native entry passed: the visible phase 03 Video launch button opened a new native window through the fixed POST route, using the exact stress-tested binary. Primary poster loaded all 96 positions; second and third video controls each played once through Ended with actual pixels. Studio was left open for review. The 92-action pass capture remains separate; packaged Slate integration, full CI and publication retain separate gates.',
        promptDescription:'Actual clean three-specimen native window opened through the visible phase 03 Video launcher. No image generation.',
        source:'V10 video_canvas.exe SHA256 1A08245B7DD2AD5ADDD1A288D4B0F119D4802AF219E53A4A8B3C71F2555799BE',
        receipt:'assets/video-studio/20261104-v10-pass/dashboard-launch.json',binaryReceipt:'assets/video-studio/20261104-v10-pass/binary-receipt.json',
        pointerReceipt:'assets/video-studio/20261104-v10-pass/native-ui-proof.json'
      }],
      studio: [
        image('01-initial', 'Initial workflow graphic', 'superseded',
          'Initial colorful ladder proposal; superseded by later iterations.',
          'Illustrate the five-stage Slate workflow, smaller builds for iteration and broader integration validation.'),
        image('02-minimal', 'Minimal workflow graphic', 'superseded',
          'Minimal palette/layout iteration; superseded by the white-card version.',
          'Simplify the graphic with a neutral palette, lightweight connectors and clear typography.'),
        image('03-white-cards-draft', 'White-card intermediate draft', 'unreviewed-draft',
          'Intermediate generated draft; no user approval recorded.',
          'Explore a white-card layout with an iteration bracket and shared production-code baseline.'),
        image('04-white-cards', 'White-card workflow graphic', 'approved-predecessor',
          'Approved base/style, superseded by the icon iteration. Visual reference only.',
          'Refine the workflow into distinct white cards with restrained text and connectors.'),
        image('05-icons', 'Workflow graphic with icons', 'approved-home-reference',
          'Current user-approved visual reference for Studio home. Does not establish native/runtime parity.',
          'Add a small identifying icon to each approved white card, retaining its layout and typography.')
      ]
    },
    nativeCanvas: {video: {report:'contract-viewer.html?document=contracts/video.md',capture:'assets/video-studio/20261104-v10-pass/dashboard-launch.jpg',alt:'V10 clean three-video native window launched from the visible phase 03 button; standalone native entry passed'}},
    summaries: { '04': { studio: integration }, '05': { studio: releaseSummary } },
    audit: {
      id: 'global-release-1.0.5-20261103', scope: 'global', retrospective: true, source,
      start, end, seconds: elapsed(start, end), note,
      unknownDurations: { writing: null, debugging: null }, perToolDurationsAvailable: false,
      sourceCommit: '9ec989b12d9dd42a15447d23e2acfccf8b92992d',
      packageSHA256: 'ba8f14befc04460832ab4326e3c0bd6d921449aedf65d04e5ede4aa946d83127',
      evidence: [release, `${run}41574895968`],
      milestones: [
        { id: 'first-release-request', at: start, source: 'Recorded first user release request.' },
        { id: 'final-code-head', at: time('02:26:26'), source: 'Recorded final code head 1c1f commit.' },
        { id: 'candidate-workflow', at: boundary, source: 'Recorded final candidate workflow start.' },
        { id: 'squash-main', at: time('03:53:43'), source: 'Squash commit 9ec989b12d9dd42a15447d23e2acfccf8b92992d.' },
        { id: 'stable-tag-run', at: time('03:53:51'), source: 'Stable workflow 41574895968.' },
        { id: 'release-published', at: time('04:53:59'), source: 'Public GitHub release published_at.' },
        { id: 'verification-complete', at: end, source: 'Recorded completed verification task.' }
      ]
    }
  };
})();
