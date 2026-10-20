// The kp-themes module chassis-rs serves at /static/kp/js/strings.js (kp-themes 11.1.1),
// copied from kp-themes js/strings.d.ts: chassis serves the module, not its types.
export type Strings = {
  /**
   * The count's name while it is still being counted (js/toggle.js) [kpadopt6-ops]
   */
  toggleCounting: string;
  /**
   * The count on a tab, while it is still being counted: its name for a screen reader (js/tabbar.js, setTabCount) [gap-tab-count]
   */
  tabCountCounting: string;
  /**
   * The count on a tab that could not be counted, in words (the dash on the tab says nothing) [gap-tab-count]
   */
  tabCountFailed: string;
  /**
   * The line said once a toggle is switched on, "Only errors on, 12 shown" [kpadopt6-ops]
   */
  toggleOn: (label: string, count: number | null) => string;
  /**
   * The line said once it is switched off [kpadopt6-ops]
   */
  toggleOff: (label: string, count: number | null) => string;
  /**
   * The line said when the count beside a toggle changed (data-kp-announce="all") [kpadopt6-ops]
   */
  toggleCount: (label: string, n: number) => string;
  /**
   * The button that switches a group's toggles off, "Clear filters (2)" [kpadopt6-ops]
   */
  toggleClear: (n: number) => string;
  /**
   * The word on a toggle whose change the app refused [kpadopt6-ops]
   */
  toggleRefused: string;
  /**
   * What a toggle's key does in the shortcut sheet [kpadopt6-ops]
   */
  toggleKeyWhat: (label: string) => string;
  /**
   * The group a toggle's key is listed under in the shortcut sheet [kpadopt6-ops]
   */
  toggleKeyGroup: string;
  /**
   * What a palette says under "Nothing matches" when it knows nothing better [kpadopt6-ops]
   */
  intentNoMatch: string;
  /**
   * A palette's line when the first word is a verb it knows but the rest names nothing it can act on [kpadopt6-ops]
   */
  intentNoSubject: (
    name: string,
    verb: string,
    subjects: string[],
    noun: string,
  ) => string;
  /**
   * The title of a palette's empty plate when no command answers [kpadopt6-ops]
   */
  paletteNothing: string;
  /**
   * A palette's status line when more commands match than are shown [kpadopt6-ops]
   */
  paletteCapped: (shown: number, total: number) => string;
  /**
   * The words under the spinner while a palette's commands are still arriving [kpadopt6-ops]
   */
  paletteLoading: string;
  /**
   * The title of a palette's plate when its commands could not be read [kpadopt6-ops]
   */
  paletteFailed: string;
  /**
   * Its sentence when the page gives no reason [kpadopt6-ops]
   */
  paletteFailedText: string;
  /**
   * Its action [kpadopt6-ops]
   */
  paletteRetry: string;
  /**
   * The word on a chip whose first number rose, and in a count: "major" (js/version-delta.js) [kpadopt6-ops]
   */
  versionMajor: string;
  /**
   * The word for a second-number jump [kpadopt6-ops]
   */
  versionMinor: string;
  /**
   * The word for a jump in a later number [kpadopt6-ops]
   */
  versionPatch: string;
  /**
   * The word for a change that has no numbers in it, such as a new digest [kpadopt6-ops]
   */
  versionOther: string;
  /**
   * The word on a chip whose number fell [kpadopt6-ops]
   */
  versionDowngrade: string;
  /**
   * The words when the two versions are the same [kpadopt6-ops]
   */
  versionSame: string;
  /**
   * The level of a chip whose versions cannot be compared [kpadopt6-ops]
   */
  versionUnknown: string;
  /**
   * The count's words when nothing is to be updated [kpadopt6-ops]
   */
  versionNoUpdates: string;
  /**
   * A chip's name while the version is still being looked up [kpadopt6-ops]
   */
  versionLoading: string;
  /**
   * The words on a chip whose lookup failed [kpadopt6-ops]
   */
  versionFailed: string;
  /**
   * The words where no version is tracked at all [kpadopt6-ops]
   */
  versionNone: string;
  /**
   * A release-notes link's extra words for a screen reader [kpadopt6-ops]
   */
  versionNotes: string;
  /**
   * What a screen reader hears for a jump, "from 10.11 to 12.1, major update" [kpadopt6-ops]
   */
  versionSpoken: (from: string, to: string, level: string) => string;
  /**
   * What it hears when nothing changes [kpadopt6-ops]
   */
  versionSpokenSame: (version: string) => string;
  /**
   * What it hears for a moving tag that only says where it goes [kpadopt6-ops]
   */
  versionSpokenTo: (to: string) => string;
  /**
   * What it hears for a version that went back [kpadopt6-ops]
   */
  versionSpokenDown: (from: string, to: string) => string;
  /**
   * What a screen reader hears for a change that has no size, "changed from latest to nightly" [kpadopt6-ops]
   */
  versionSpokenChanged: (from: string, to: string) => string;
  /**
   * The release-notes link's words on an acknowledge gate [kpadopt6-ops]
   */
  ackLink: string;
  /**
   * The tick's words when the page names none [kpadopt6-ops]
   */
  ackLabel: string;
  /**
   * The Retry button of a gate whose notes could not be found [kpadopt6-ops]
   */
  ackRetry: string;
  /**
   * Why the tick is off while the notes are looked up [kpadopt6-ops]
   */
  ackLooking: string;
  /**
   * Why the tick is locked while the job runs [kpadopt6-ops]
   */
  ackLocked: string;
  /**
   * Why the tick is off until the notes were opened [kpadopt6-ops]
   */
  ackOpenFirst: string;
  /**
   * Why the guarded button is off, with the tick's own words [kpadopt6-ops]
   */
  ackReason: (label: string) => string;
  /**
   * Why it is off when several gates guard it, "2 of 3 release notes still to read" [kpadopt6-ops]
   */
  ackReasonMany: (remaining: number, total: number) => string;
  /**
   * Said once when the last gate is ticked and the button comes on [kpadopt6-ops]
   */
  ackEnabled: string;
  /**
   * Said once when a tick is taken away [kpadopt6-ops]
   */
  ackDisabled: (remaining: number) => string;
  /**
   * A gate's line when no release notes link was found [kpadopt6-ops]
   */
  ackNoLink: string;
  /**
   * The list's default heading, the exact count (js/deplist.js) [kpadopt6-ops]
   */
  deplistCount: (n: number) => string;
  /**
   * The heading while the rows are still being read [kpadopt6-ops]
   */
  deplistReading: string;
  /**
   * A row as a screen reader hears it, "paperless needs postgres" [kpadopt6-ops]
   */
  deplistNeeds: (from: string, to: string) => string;
  /**
   * The same for a list that says what reaches what [kpadopt6-ops]
   */
  deplistReaches: (from: string, to: string) => string;
  /**
   * The word between the names of a cycle, "paperless needs redis needs paperless" [kpadopt6-ops]
   */
  deplistNeedsWord: string;
  /**
   * The button that unfolds the rest, with the exact number of rows [kpadopt6-ops]
   */
  deplistMore: (n: number) => string;
  /**
   * The button that folds it again [kpadopt6-ops]
   */
  deplistFewer: string;
  /**
   * The empty plate's title when the page gives no sentence [kpadopt6-ops]
   */
  deplistEmpty: string;
  /**
   * The failed plate's title [kpadopt6-ops]
   */
  deplistFailed: string;
  /**
   * Its sentence when the page gives no reason [kpadopt6-ops]
   */
  deplistFailedText: string;
  /**
   * Its action [kpadopt6-ops]
   */
  deplistRetry: string;
  /**
   * The words under the busy card's spinner [kpadopt6-ops]
   */
  deplistLoading: string;
  /**
   * The word for a needed thing that is running [kpadopt6-ops]
   */
  deplistRunning: string;
  /**
   * The word for a needed thing that is stopped [kpadopt6-ops]
   */
  deplistStopped: string;
  /**
   * The word for a needed thing whose state is not known [kpadopt6-ops]
   */
  deplistUnknown: string;
  /**
   * The button that copies the rows as text [kpadopt6-ops]
   */
  deplistCopy: string;
  /**
   * Its answer once the rows are on the clipboard [kpadopt6-ops]
   */
  deplistCopied: string;
  /**
   * The hop list's accessible name [kpadopt6-ops]
   */
  traceLabel: string;
  /**
   * The summary line when every hop lets the connection through [kpadopt6-ops]
   */
  traceAllowed: string;
  /**
   * The summary line when a hop refuses it, with the deciding rule when there is one [kpadopt6-ops]
   */
  traceBlocked: (hop: string, rule: string) => string;
  /**
   * The summary line when a hop could not be measured [kpadopt6-ops]
   */
  traceNotAll: string;
  /**
   * A hop's verdict word, allowed [kpadopt6-ops]
   */
  traceAllowedWord: string;
  /**
   * A hop's verdict word, refused [kpadopt6-ops]
   */
  traceRefusedWord: string;
  /**
   * The word for a hop after a refusal, which the connection never reaches [kpadopt6-ops]
   */
  traceNotReached: string;
  /**
   * The word for a hop whose verdict is not known [kpadopt6-ops]
   */
  traceNotMeasured: string;
  /**
   * The deciding rule's chip, "Rule IoT to media" [kpadopt6-ops]
   */
  traceRule: (rule: string) => string;
  /**
   * The words under the busy card's spinner while the path is checked [kpadopt6-ops]
   */
  traceRunning: string;
  /**
   * The empty plate's title before a question was asked [kpadopt6-ops]
   */
  traceEmpty: string;
  /**
   * Its sentence [kpadopt6-ops]
   */
  traceEmptyText: string;
  /**
   * The failed plate's title [kpadopt6-ops]
   */
  traceFailed: string;
  /**
   * Its sentence when the page gives no reason [kpadopt6-ops]
   */
  traceFailedText: string;
  /**
   * Its action [kpadopt6-ops]
   */
  traceRetry: string;
  /**
   * The button that copies the trace as text [kpadopt6-ops]
   */
  traceCopy: string;
  /**
   * Its answer once the text is on the clipboard [kpadopt6-ops]
   */
  traceCopied: string;
  /**
   * The button that swaps source and target [kpadopt6-ops]
   */
  traceSwap: string;
  /**
   * What the swap key does in the shortcut sheet [kpadopt6-ops]
   */
  traceKeySwap: string;
  /**
   * The group the swap key is listed under [kpadopt6-ops]
   */
  traceKeyGroup: string;
  /**
   * A card's tick, named for a screen reader, "Select media" [kpadopt6-ops]
   */
  entitySelect: (name: string) => string;
  /**
   * The state word of a card that could not be read [kpadopt6-ops]
   */
  entityFailed: string;
  /**
   * What a card says where it has nothing measured [kpadopt6-ops]
   */
  entityNotMeasured: string;
  /**
   * The "+2" button's accessible name [kpadopt6-ops]
   */
  entityMoreFlags: (n: number) => string;
  /**
   * The name of the menu the "+2" opens [kpadopt6-ops]
   */
  entityFlagsLabel: string;
  /**
   * A failed card's or grid's Retry [kpadopt6-ops]
   */
  entityRetry: string;
  /**
   * The button that opens the rest of a card's actions [kpadopt6-ops]
   */
  entityMore: string;
  /**
   * The name of that menu [kpadopt6-ops]
   */
  entityMoreLabel: string;
  /**
   * The words under the busy card's spinner while the cards are read [kpadopt6-ops]
   */
  entityLoading: string;
  /**
   * The failed plate's title for a grid of cards [kpadopt6-ops]
   */
  entityGridFailed: string;
  /**
   * Its sentence when the page gives no reason [kpadopt6-ops]
   */
  entityGridFailedText: string;
  /**
   * The empty plate's title for a grid with no cards [kpadopt6-ops]
   */
  entityGridEmpty: string;
  /**
   * The empty plate's action [kpadopt6-ops]
   */
  entityNew: string;
  /**
   * The favourite button's name, "Favourite jellyfin" [gap-entity-tiles-1019]
   */
  entityStar: (name: string) => string;
  /**
   * What a screen reader adds to a link that opens a new tab [gap-entity-tiles-1019]
   */
  entityOpensNew: string;
  /**
   * The trend row's label when the data gives none [gap-entity-tiles-1019]
   */
  entityTrend: string;
  /**
   * What the trend row says where there are fewer than two readings [gap-entity-tiles-1019]
   */
  entityTrendNone: string;
  /**
   * The trend line's text alternative, "CPU, last 24 h, now 34%" [gap-entity-tiles-1019]
   */
  entityTrendName: (label: string, text: string) => string;
  /**
   * What a tile says where its app could not be read [gap-entity-tiles-1019]
   */
  entityTileFailed: string;
  /**
   * The busy card's words while a board of tiles is read [gap-entity-tiles-1019]
   */
  entityTileLoading: string;
  /**
   * The failed plate's title for a board of tiles [gap-entity-tiles-1019]
   */
  entityTileGridFailed: string;
  /**
   * The empty plate's title for a board with no tile [gap-entity-tiles-1019]
   */
  entityTileGridEmpty: string;
  /**
   * The empty board's action [gap-entity-tiles-1019]
   */
  entityTileNew: string;
  alertSuccess: string;
  alertWarning: string;
  alertInfo: string;
  alertError: string;
  /**
   * The word a screen reader hears before a critical problem on the attention band (setAttention) [scope-143]
   */
  attentionCritical: string;
  /**
   * The same, before a warning [scope-143]
   */
  attentionWarning: string;
  /**
   * The same, before an information [scope-143]
   */
  attentionInfo: string;
  /**
   * A ticking freshness line, "updated 12 s ago" (js/freshness.js) [scope-143]
   */
  agoText: (verb: string, duration: string) => string;
  /**
   * The same line with no moment yet, "not updated yet" [scope-143]
   */
  agoNever: (verb: string) => string;
  /**
   * A duration's seconds part, "12 s" [scope-143]
   */
  agoSeconds: (n: number) => string;
  /**
   * Its minutes part, "2 min" [scope-143]
   */
  agoMinutes: (n: number) => string;
  /**
   * Its hours part, "3 h" [scope-143]
   */
  agoHours: (n: number) => string;
  /**
   * Its days part, "1 day", "2 days" [scope-143]
   */
  agoDays: (n: number) => string;
  /**
   * The verb a freshness line uses when its element names none [scope-143]
   */
  agoVerb: string;
  /**
   * What a freshness line with `data-kp-ago-announce="state"` announces once it turns stale [scope-143]
   */
  agoStale: (text: string) => string;
  /**
   * What it announces once it is fresh again [scope-143]
   */
  agoFresh: (text: string) => string;
  /**
   * The console warning for a ticking line inside a live region [scope-143]
   */
  agoInLiveRegion: string;
  /**
   * A step list's waiting step, as a screen reader hears it (js/steplist.js)
   */
  stepPending: string;
  /**
   * The step being run
   */
  stepRunning: string;
  /**
   * A finished step
   */
  stepDone: string;
  /**
   * A step that failed
   */
  stepFailed: string;
  /**
   * A step that was left out
   */
  stepSkipped: string;
  /**
   * The step a journey stands at now, neither running nor done (`data-state="current"`) [kpadopt4]
   */
  stepCurrent: string;
  /**
   * What the list's status region says when a step changes, "Pull the images: running"
   */
  stepAnnounce: (title: string, state: string) => string;
  /**
   * A status row that is fine, as a screen reader hears it (js/statusrow.js)
   */
  statusOk: string;
  /**
   * A status row still being read
   */
  statusLoading: string;
  /**
   * A status row that failed
   */
  statusFailed: string;
  /**
   * A status row with nothing to show
   */
  statusEmpty: string;
  /**
   * A status row that passes with a reservation, `data-state="warn"` [kpadopt4-rows]
   */
  statusWarn: string;
  /**
   * What a loading card's busy card says under its spinner (js/card-state.js) [kpadopt4-rows]
   */
  cardLoading: string;
  /**
   * An empty card's note when the page gives no title [kpadopt4-rows]
   */
  cardEmpty: string;
  /**
   * A failed card's note when the page gives no title [kpadopt4-rows]
   */
  cardFailed: string;
  /**
   * The Try again button of a failed card [kpadopt4-rows]
   */
  cardRetry: string;
  /**
   * The Retry button of a failed status row
   */
  statusRetry: string;
  /**
   * That button's accessible name, "Retry vault"
   */
  statusRetryLabel: (label: string) => string;
  /**
   * What the list's status region says when a row changes, "vault: failed, The host did not answer"
   */
  statusAnnounce: (label: string, state: string, detail: string) => string;
  /**
   * What it says when more than three rows change in one tick, "12 updated, 2 failed"
   */
  statusAnnounceMany: (count: number, failed: number) => string;
  /**
   * The "only" button's text on a facet row (js/facets.js)
   */
  facetOnly: string;
  /**
   * That button's accessible name, "Show only Host"
   */
  facetOnlyLabel: (label: string) => string;
  /**
   * The "Everything" row's text when the group gives none
   */
  facetAll: string;
  /**
   * What a facet group's status region says after "only", "Showing only Host"
   */
  facetShowingOnly: (label: string) => string;
  /**
   * What it says after "Everything"
   */
  facetShowingAll: string;
  /**
   * The glyph of an identity mark with no name (js/idmark.js) [kpadopt4-data]
   */
  idmarkUnknown: string;
  /**
   * A mark's status word, online (js/idmark.js) [kpadopt5-people]
   */
  idmarkOnline: string;
  /**
   * A mark's status word, away
   */
  idmarkAway: string;
  /**
   * A mark's status word, busy
   */
  idmarkBusy: string;
  /**
   * A mark with a status as a screen reader hears it, "Ada, online"
   */
  idmarkLabel: (name: string, status: string) => string;
  /**
   * A group of marks, named by its size, "5 people"
   */
  idmarkGroup: (count: number) => string;
  /**
   * The "+N" button of a group, "Show the 3 more people"
   */
  idmarkMore: (count: number) => string;
  /**
   * The heading of the list the "+N" opens, "3 more people"
   */
  idmarkMoreTitle: (count: number) => string;
  /**
   * What the busy card over the sign-in methods says (js/passkeys.js) [kpadopt5-people]
   */
  passkeysLoading: string;
  /**
   * The sign-in methods list's name for a screen reader
   */
  passkeysListLabel: string;
  /**
   * The empty sign-in methods list
   */
  passkeysEmptyTitle: string;
  /**
   * Its sentence, what a passkey is for
   */
  passkeysEmptyText: string;
  /**
   * The Register button of the empty list
   */
  passkeysRegister: string;
  /**
   * A sign-in methods list that could not be read, when the page gives no sentence
   */
  passkeysError: string;
  /**
   * The Retry button of a failed list
   */
  passkeysRetry: string;
  /**
   * The word for a kind of way in: passkey, magic link, password, token
   */
  passkeysKind: (kind: string) => string;
  /**
   * When a way was added, "Added 12/10/2026"
   */
  passkeysAdded: (date: string) => string;
  /**
   * The verb of the last-used line, "Last used" (then "3 days ago")
   */
  passkeysLastUsedVerb: string;
  /**
   * A way that was never used
   */
  passkeysNeverUsed: string;
  /**
   * The badge on the way this session signed in with
   */
  passkeysCurrent: string;
  /**
   * Its tooltip
   */
  passkeysCurrentTitle: string;
  /**
   * The badge on a way unused for long, "Unused for 120 days"
   */
  passkeysUnused: (days: number) => string;
  /**
   * Its tooltip: a candidate to remove
   */
  passkeysUnusedTitle: string;
  /**
   * The Rename button
   */
  passkeysRename: string;
  /**
   * Rename, as a screen reader hears it with the way's name
   */
  passkeysRenameOf: (label: string) => string;
  /**
   * The name field's label while renaming
   */
  passkeysRenameLabel: string;
  /**
   * Save a new name
   */
  passkeysSave: string;
  /**
   * Leave renaming
   */
  passkeysCancel: string;
  /**
   * The line under a name left empty
   */
  passkeysNameRequired: string;
  /**
   * Announced after a rename
   */
  passkeysRenamed: (label: string) => string;
  /**
   * A rename the server refused, with its reason
   */
  passkeysRenameRefused: (why: string) => string;
  /**
   * The Remove button
   */
  passkeysRemove: string;
  /**
   * The Remove button while it works
   */
  passkeysRemoving: string;
  /**
   * Remove, as a screen reader hears it with the way's name
   */
  passkeysRemoveOf: (label: string) => string;
  /**
   * The confirmation's question, "Remove Work laptop?"
   */
  passkeysRemoveTitle: (label: string) => string;
  /**
   * The first line of what a removal loses
   */
  passkeysLosesWay: (label: string) => string;
  /**
   * What removing the only way in loses
   */
  passkeysLosesLast: string;
  /**
   * What removing the way in use loses
   */
  passkeysLosesCurrent: string;
  /**
   * Announced after a removal
   */
  passkeysRemoved: (label: string) => string;
  /**
   * A removal the server refused, "Not removed: why"
   */
  passkeysRefused: (why: string) => string;
  /**
   * The reason when a refusal gave none
   */
  passkeysRefusedFallback: string;
  /**
   * The reason Remove is off on the only way in, and what to do first
   */
  passkeysLastWay: string;
  /**
   * The short note in the place of Remove on the only way in (the `note` look)
   */
  passkeysOnlyWay: string;
  /**
   * The warning on the only way in when removing it is allowed
   */
  passkeysLastWayWarn: string;
  /**
   * The warning heading when the page is not secure
   */
  passkeysInsecureTitle: string;
  /**
   * What to do about it
   */
  passkeysInsecureText: string;
  /**
   * What the busy card over a conversation says (js/thread.js) [kpadopt5-people]
   */
  threadLoading: string;
  /**
   * The conversation's name for a screen reader
   */
  threadLabel: string;
  /**
   * The empty conversation
   */
  threadEmptyTitle: string;
  /**
   * Its sentence
   */
  threadEmptyText: string;
  /**
   * The Add button of the empty conversation
   */
  threadAdd: string;
  /**
   * A conversation that could not be read, when the page gives no sentence
   */
  threadError: string;
  /**
   * The Retry button of a failed conversation, and of a message not sent
   */
  threadRetry: string;
  /**
   * The sender of an outgoing message with no name
   */
  threadYou: string;
  /**
   * The sender of an incoming message with no name
   */
  threadUnknownSender: string;
  /**
   * A message's label, "From Acme HR, 13/10/2026 14:02"
   */
  threadBubbleLabel: (sender: string, moment: string) => string;
  /**
   * A day divider, "Monday 13/10/2026"
   */
  threadDay: (weekday: string, date: string) => string;
  /**
   * The attachments list's name, "2 attachments"
   */
  threadAttachments: (count: number) => string;
  /**
   * The button under a folded message, "Show all 48 lines"
   */
  threadShowAll: (lines: number) => string;
  /**
   * The same button when the message is whole
   */
  threadShowLess: string;
  /**
   * The Copy text button of a message
   */
  threadCopy: string;
  /**
   * Copy, as a screen reader hears it with the sender
   */
  threadCopyOf: (sender: string) => string;
  /**
   * The Copy button when the clipboard refused
   */
  threadCopyFailed: string;
  /**
   * The line on a message being sent
   */
  threadSending: string;
  /**
   * The line on a message that was not sent, "Not sent: why"
   */
  threadNotSent: (why: string) => string;
  /**
   * The reason when none was given
   */
  threadNotSentFallback: string;
  /**
   * The Jump to newest button
   */
  threadJump: string;
  /**
   * The words in front of a median's value, "median" (js/chart.js) [kpadopt5-people]
   */
  chartMedian: string;
  /**
   * The sentence over a chart with too few readings, "3 of 7 days needed before a trend means something"
   */
  chartFew: (have: number, need: number, unit: string) => string;
  /**
   * What the readings are called in it when the page names none
   */
  chartFewUnit: string;
  /**
   * The key above a chart's plot, for a screen reader
   */
  chartAnnotationKey: string;
  /**
   * The key's word for an event that has a tone and no kind
   */
  chartKindTone: (tone: string) => string;
  /**
   * The key's note on a ringed run
   */
  chartRunsKey: string;
  /**
   * The tooltip of a band, "Maintenance, from … to …"
   */
  chartMarkBand: (label: string, from: string, until: string) => string;
  /**
   * The time of a band in the tooltip, "14:00 to 15:30"
   */
  chartBandWhen: (from: string, until: string) => string;
  /**
   * The arrow for events before the window, "2 events before this window"
   */
  chartEdgeBefore: (count: number) => string;
  /**
   * The arrow for events after the window
   */
  chartEdgeAfter: (count: number) => string;
  /**
   * A key item's tooltip, "Show or hide update events"
   */
  chartKindOn: (kind: string) => string;
  /**
   * The legend row of a share bar's free rest (js/stackbar.js)
   */
  stackbarRest: string;
  /**
   * The legend row the parts too small to draw are merged into
   */
  stackbarOther: string;
  /**
   * One part, as a screen reader and the hover card read it
   */
  stackbarPart: (label: string, value: string, percent: string) => string;
  /**
   * The free row when the parts add up to more than the whole
   */
  stackbarOver: (value: string) => string;
  /**
   * The bar's accessible name: every part in order
   */
  stackbarLabel: (parts: string[]) => string;
  /**
   * A share bar being read
   */
  stackbarLoading: string;
  /**
   * A share bar with nothing read yet
   */
  stackbarEmpty: string;
  /**
   * A share bar that could not be read
   */
  stackbarError: string;
  /**
   * The Retry button of a failed share bar
   */
  stackbarRetry: string;
  /**
   * What the busy card over a bullet list says (js/bullets.js)
   */
  bulletsLoading: string;
  /**
   * A bullet list with no rows, when the page gives no reason
   */
  bulletsEmpty: string;
  /**
   * A bullet list that could not be read, when the page gives no sentence
   */
  bulletsError: string;
  /**
   * The Retry button of a failed bullet list
   */
  bulletsRetry: string;
  /**
   * A row with nothing to measure, for a screen reader
   */
  bulletsUnknown: string;
  /**
   * A row's meter as a screen reader reads it
   */
  bulletsValueText: (figure: string, threshold: string) => string;
  /**
   * A code block's accessible name when it has no title (js/code.js)
   */
  codeLabel: string;
  /**
   * The code block's Wrap toggle
   */
  codeWrap: string;
  /**
   * The JSON view laid out with indents
   */
  codePretty: string;
  /**
   * The JSON view as it came
   */
  codeRaw: string;
  /**
   * The accessible name of the Pretty / Raw choice
   */
  codeFormat: string;
  /**
   * The button under a folded block
   */
  codeShowAll: (lines: number) => string;
  /**
   * The same button once the block is open
   */
  codeShowLess: string;
  /**
   * The block's line count beside its title
   */
  codeLines: (lines: number) => string;
  /**
   * A block with nothing in it
   */
  codeEmpty: string;
  /**
   * A block whose content is not text
   */
  codeBinary: (size: string) => string;
  /**
   * The foot of a block that holds only the start
   */
  codeTruncated: (shown: string, total: string) => string;
  /**
   * What the busy card over a block says
   */
  codeLoading: string;
  /**
   * A block that could not be read, when the page gives no sentence
   */
  codeError: string;
  /**
   * Announced after a copy, naming the block
   */
  codeCopiedAnnouncement: (title: string) => string;
  /**
   * Said after a tick, a range or a clear: how many rows are selected (js/row-select.js) [kpadopt5]
   */
  rowSelectCount: (n: number) => string;
  /**
   * Said after a range that passed rows that cannot be selected [kpadopt5]
   */
  rowSelectSkipped: (n: number) => string;
  /**
   * Said after Ctrl+A: every row the table shows is ticked [kpadopt5]
   */
  rowSelectAllShown: (n: number) => string;
  /**
   * Said when the cursor moves to a row, "INC-4471, row 3 of 12" [kpadopt5]
   */
  rowSelectCursor: (label: string, at: number, of: number) => string;
  /**
   * Said when a long press starts selection mode on a phone [kpadopt5]
   */
  rowSelectMode: string;
  /**
   * Said when selection mode ends [kpadopt5]
   */
  rowSelectModeEnd: string;
  /**
   * The hint line under a selectable table: j and k [kpadopt5]
   */
  rowKeysMove: string;
  /**
   * The hint line: x [kpadopt5]
   */
  rowKeysTick: string;
  /**
   * The hint line: Shift and x [kpadopt5]
   */
  rowKeysRange: string;
  /**
   * The hint line: Esc [kpadopt5]
   */
  rowKeysClear: string;
  /**
   * The hint line: Enter [kpadopt5]
   */
  rowKeysOpen: string;
  /**
   * The hint line: Ctrl and A, when the table offers it [kpadopt5]
   */
  rowKeysAll: string;
  /**
   * The accessible name of a selection bar (js/selection-bar.js) [kpadopt5]
   */
  selectionLabel: string;
  /**
   * The bar's count, "3 selected" [kpadopt5]
   */
  selectionCount: (n: number) => string;
  /**
   * The bar's button that ticks nothing: it says what it does [kpadopt5]
   */
  selectionClear: string;
  /**
   * Its tooltip, naming the key [kpadopt5]
   */
  selectionClearTitle: string;
  /**
   * The names line when not every name fits, "+4 more" with the exact number [kpadopt5]
   */
  selectionMore: (n: number) => string;
  /**
   * The same button once the whole list is open [kpadopt5]
   */
  selectionFewer: string;
  /**
   * The phone's button that opens the bar's actions [kpadopt5]
   */
  selectionActions: string;
  /**
   * The offer beyond what is shown, "Select all 312 that match" [kpadopt5]
   */
  selectionMatching: (total: number) => string;
  /**
   * The bar's line once every match is selected [kpadopt5]
   */
  selectionMatchingOn: (total: number) => string;
  /**
   * The button that takes the offer back, "Select only the 40 shown" [kpadopt5]
   */
  selectionOnlyShown: (n: number) => string;
  /**
   * Said when a bar action starts, "Backing up 3…" is the page's own words; this is the fallback [kpadopt5]
   */
  selectionBusy: (words: string) => string;
  /**
   * The bar's line after an action some rows refused, "2 done, 1 refused" [kpadopt5]
   */
  selectionResult: (done: number, refused: number) => string;
  /**
   * Said when selected rows are gone after a refresh [kpadopt5]
   */
  selectionDropped: (n: number) => string;
  /**
   * The reason an action is off for some rows, "2 of 3 can be updated" with the page's verb after it [kpadopt5]
   */
  selectionCan: (can: number, total: number) => string;
  /**
   * Said when the selection is cleared [kpadopt5]
   */
  selectionCleared: string;
  /**
   * The Undo button of an undo toast (js/undo-toast.js) [kpadopt5]
   */
  undoToastUndo: string;
  /**
   * Its tooltip, naming the key [kpadopt5]
   */
  undoToastKeyTitle: (key: string) => string;
  /**
   * Said once when the toast opens, "Deleted. Undo available for 6 seconds" [kpadopt5]
   */
  undoToastAnnounce: (text: string, seconds: number) => string;
  /**
   * The text clock, "5 s" [kpadopt5]
   */
  undoToastClock: (seconds: number) => string;
  /**
   * The toast's line after an undo [kpadopt5]
   */
  undoToastRestored: string;
  /**
   * The toast's line while the commit is on its way [kpadopt5]
   */
  undoToastSaving: string;
  /**
   * The toast's line when the commit failed, "Not done: why" [kpadopt5]
   */
  undoToastFailed: (why: string) => string;
  /**
   * The accessible name of the toast's close button [kpadopt5]
   */
  undoToastDismiss: string;
  /**
   * The word for one new row on a live list (js/newpill.js) [kpadopt5]
   */
  newpillUnitOne: string;
  /**
   * The word for several [kpadopt5]
   */
  newpillUnitMany: string;
  /**
   * The pill, "12 new lines" [kpadopt5]
   */
  newpillNew: (n: number, unit: string) => string;
  /**
   * The pill's second half on a list that grows at the end [kpadopt5]
   */
  newpillBackTail: string;
  /**
   * The pill's second half on a list that grows at the top [kpadopt5]
   */
  newpillBackHead: string;
  /**
   * Said at most every ten seconds while the list is read elsewhere [kpadopt5]
   */
  newpillAnnounce: (n: number, unit: string) => string;
  /**
   * The toolbar button that stops following [kpadopt5]
   */
  newpillPause: string;
  /**
   * The toolbar button that follows again, shown while paused [kpadopt5]
   */
  newpillResume: string;
  /**
   * The divider at the first unseen row, "New since 14:32" [kpadopt5]
   */
  newpillSince: (time: string) => string;
  /**
   * Said when following is paused [kpadopt5]
   */
  newpillPausedNote: string;
  /**
   * Said when following resumes [kpadopt5]
   */
  newpillFollowing: string;
  /**
   * A payload cell with nothing in it (js/code.js, cell) [kpadopt5]
   */
  payloadEmpty: string;
  /**
   * A payload cell that holds binary data, "binary, 2.1 kB" [kpadopt5]
   */
  payloadBinary: (size: string) => string;
  /**
   * The button that opens a long payload in a dialog [kpadopt5]
   */
  payloadOpen: string;
  /**
   * The link that downloads a binary payload when the page gives a URL [kpadopt5]
   */
  payloadDownload: string;
  /**
   * The cell's fold button, "Show all 40 lines" [kpadopt5]
   */
  payloadShowAll: (lines: number) => string;
  /**
   * The cell's copy button: it copies the whole value, folded or not [kpadopt5]
   */
  payloadCopyFull: string;
  /**
   * The quiet line under a valid object, "Valid JSON · 3 keys" [kpadopt5]
   */
  jsonValidKeys: (n: number) => string;
  /**
   * The same for a list [kpadopt5]
   */
  jsonValidItems: (n: number) => string;
  /**
   * The same for a bare value [kpadopt5]
   */
  jsonValidValue: string;
  /**
   * The error line, "Line 3, column 12: expected ','" [kpadopt5]
   */
  jsonInvalid: (line: number, column: number, why: string) => string;
  /**
   * The error line when the place is not known [kpadopt5]
   */
  jsonInvalidPlain: (why: string) => string;
  /**
   * The line under text that is not JSON where plain text is allowed [kpadopt5]
   */
  jsonPlainText: string;
  /**
   * The line under an empty field [kpadopt5]
   */
  jsonEmpty: string;
  /**
   * The Format button [kpadopt5]
   */
  jsonFormat: string;
  /**
   * The Minify button [kpadopt5]
   */
  jsonMinify: string;
  /**
   * The Wrap toggle [kpadopt5]
   */
  jsonWrap: string;
  /**
   * The Copy button [kpadopt5]
   */
  jsonCopy: string;
  /**
   * The help line under the field: how to leave it with the keyboard [kpadopt5]
   */
  jsonTabHint: string;
  /**
   * The accessible name of the line numbers [kpadopt5]
   */
  jsonGutter: string;
  /**
   * The validity message that stops a form with broken JSON [kpadopt5]
   */
  jsonBlocked: string;
  /**
   * The error line when the page's shape check refuses valid JSON [kpadopt5]
   */
  jsonSchemaFailed: (why: string) => string;
  /**
   * Said after Format [kpadopt5]
   */
  jsonFormatted: string;
  /**
   * The accessible name of the request list (js/request-inspector.js) [kpadopt5]
   */
  reqLabel: string;
  /**
   * The empty list's heading [kpadopt5]
   */
  reqEmptyTitle: string;
  /**
   * Its sentence [kpadopt5]
   */
  reqEmptyText: string;
  /**
   * Its button [kpadopt5]
   */
  reqSendTest: string;
  /**
   * The failed list's heading [kpadopt5]
   */
  reqFailedTitle: string;
  /**
   * The busy card's words [kpadopt5]
   */
  reqLoading: string;
  /**
   * The headers block's heading [kpadopt5]
   */
  reqHeaders: string;
  /**
   * The body block's title [kpadopt5]
   */
  reqBody: string;
  /**
   * A request without a body [kpadopt5]
   */
  reqNoBody: string;
  /**
   * A request without headers [kpadopt5]
   */
  reqNoHeaders: string;
  /**
   * Copy as curl [kpadopt5]
   */
  reqCopyCurl: string;
  /**
   * Copy as JSON [kpadopt5]
   */
  reqCopyJson: string;
  /**
   * Copy the body [kpadopt5]
   */
  reqCopyBody: string;
  /**
   * The separate button that copies the curl with the real secrets [kpadopt5]
   */
  reqCopyCurlSecrets: string;
  /**
   * Said after a copy, "curl copied, secrets masked" is built from this and the next [kpadopt5]
   */
  reqCopyDone: (what: string) => string;
  /**
   * Added to a copy that masked the secrets [kpadopt5]
   */
  reqMaskedNote: string;
  /**
   * The button that shows one masked header on screen [kpadopt5]
   */
  reqSecretShow: string;
  /**
   * The same, once shown [kpadopt5]
   */
  reqSecretHide: string;
  /**
   * What a screen reader hears for a masked value [kpadopt5]
   */
  reqSecretMasked: string;
  /**
   * The button under the list, "Show 12 older" [kpadopt5]
   */
  reqShowOlder: (n: number) => string;
  /**
   * The accessible name of the status filter [kpadopt5]
   */
  reqFilterLabel: string;
  /**
   * The filter's first choice [kpadopt5]
   */
  reqFilterAll: string;
  /**
   * The word beside a 2xx status [kpadopt5]
   */
  reqToneSuccess: string;
  /**
   * The word beside a 3xx status [kpadopt5]
   */
  reqToneInfo: string;
  /**
   * The word beside a 4xx status [kpadopt5]
   */
  reqToneWarning: string;
  /**
   * The word beside a 5xx status [kpadopt5]
   */
  reqToneError: string;
  /**
   * The word beside a request without a status [kpadopt5]
   */
  reqToneNone: string;
  /**
   * The badge of a request the page sent itself [kpadopt5]
   */
  reqSourceTest: string;
  /**
   * The line above the list, "20 of 31 requests" [kpadopt5]
   */
  reqCount: (shown: number, total: number) => string;
  /**
   * The list when the filter shows nothing [kpadopt5]
   */
  reqNoMatch: string;
  /**
   * A request's accessible name [kpadopt5]
   */
  reqSummary: (method: string, path: string, status: string) => string;
  /**
   * The counter button's word (js/notices.js) [kpadopt5-notices]
   */
  noticesInbox: string;
  /**
   * The panel's heading [kpadopt5-notices]
   */
  noticesTitle: string;
  /**
   * The Inbox page's heading [kpadopt5-notices]
   */
  noticesPageTitle: string;
  /**
   * The accessible name of the list of notices [kpadopt5-notices]
   */
  noticesList: string;
  /**
   * The counter's label, "5 unread" [kpadopt5-notices]
   */
  noticesUnread: (n: number) => string;
  /**
   * The counter's label with nothing unread [kpadopt5-notices]
   */
  noticesNoneUnread: string;
  /**
   * The counter's label when the notices could not be read [kpadopt5-notices]
   */
  noticesCounterError: (why: string) => string;
  /**
   * The counter button's accessible name, "Inbox: 5 unread. Quiet until 09:00" [kpadopt5-notices]
   */
  noticesCounterName: (inbox: string, label: string, quiet: string) => string;
  /**
   * What the busy card over the list says, and the counter's label while it loads [kpadopt5-notices]
   */
  noticesLoading: string;
  /**
   * The quiet line, "Quiet until 09:00" [kpadopt5-notices]
   */
  noticesQuiet: (time: string) => string;
  /**
   * The button that ends the quiet time [kpadopt5-notices]
   */
  noticesEndQuiet: string;
  /**
   * The verb of a notice's age line, "since 2 min ago" [kpadopt5-notices]
   */
  noticesSinceVerb: string;
  /**
   * The per-row button that marks a notice read [kpadopt5-notices]
   */
  noticesMarkRead: string;
  /**
   * The per-row button that marks it unread again [kpadopt5-notices]
   */
  noticesMarkUnread: string;
  /**
   * The button that marks every notice read [kpadopt5-notices]
   */
  noticesReadAll: string;
  /**
   * The footer link, "All 37 in the Inbox", with the exact count [kpadopt5-notices]
   */
  noticesAllInInbox: (n: number) => string;
  /**
   * The empty plate's title [kpadopt5-notices]
   */
  noticesEmpty: string;
  /**
   * Its sentence [kpadopt5-notices]
   */
  noticesEmptyText: string;
  /**
   * The failed plate's title [kpadopt5-notices]
   */
  noticesErrorTitle: string;
  /**
   * Its sentence when the page gives no reason [kpadopt5-notices]
   */
  noticesErrorText: string;
  /**
   * The failed plate's Retry button [kpadopt5-notices]
   */
  noticesRetry: string;
  /**
   * The title of the per-source switches [kpadopt5-notices]
   */
  noticesSources: string;
  /**
   * A muted source's state word [kpadopt5-notices]
   */
  noticesMuted: string;
  /**
   * A source that is on [kpadopt5-notices]
   */
  noticesNotMuted: string;
  /**
   * Announced when one new notice arrived [kpadopt5-notices]
   */
  noticesNew: (title: string) => string;
  /**
   * Announced when several arrived, naming the worst [kpadopt5-notices]
   */
  noticesNewMany: (n: number, title: string) => string;
  /**
   * The default first answer (js/question-card.js) [kpadopt5-question-card]
   */
  questionAllow: string;
  /**
   * The default second answer [kpadopt5-question-card]
   */
  questionStop: string;
  /**
   * The accessible name of the region that holds the open questions [kpadopt5-question-card]
   */
  questionsLabel: string;
  /**
   * How long a question still waits, "Waits 1 min 12 s more" [kpadopt5-question-card]
   */
  questionLeft: (duration: string) => string;
  /**
   * The time line when the time ran out [kpadopt5-question-card]
   */
  questionTimeUp: string;
  /**
   * What happens at zero, "If nobody answers: Stop." [kpadopt5-question-card]
   */
  questionDefault: (label: string) => string;
  /**
   * Announced once when the time gets short [kpadopt5-question-card]
   */
  questionUrgent: (duration: string) => string;
  /**
   * The outcome of a question nobody answered, when there is no default answer [kpadopt5-question-card]
   */
  questionNobody: string;
  /**
   * The outcome when the default answer applied [kpadopt5-question-card]
   */
  questionNobodyDefault: (label: string) => string;
  /**
   * The outcome of an answer that has no sentence of its own [kpadopt5-question-card]
   */
  questionAnswered: (label: string) => string;
  /**
   * The note when the app could not send the answer, "Not sent: why. Nothing changed; answer again." [kpadopt5-question-card]
   */
  questionRefused: (why: string) => string;
  /**
   * The button that takes a finished card away [kpadopt5-question-card]
   */
  questionDismiss: string;
  /**
   * The tab title's prefix while questions wait, "(1) waiting" [kpadopt5-question-card]
   */
  questionWaiting: (count: number) => string;
  /**
   * The accessible name of a transfer's bar when it has no file name (js/transfer.js) [kpadopt5-transfer]
   */
  transferBar: string;
  /**
   * The reading of a transfer with a known size, "1.2 GB of 4.1 GB" [kpadopt5-transfer]
   */
  transferOf: (done: string, total: string) => string;
  /**
   * The reading of a transfer of unknown size, "1.2 GB so far" [kpadopt5-transfer]
   */
  transferSoFar: (done: string) => string;
  /**
   * The time column of a transfer whose total size is not known, beside "1.2 GB so far" [kpadopt5-r3]
   */
  transferSizeUnknown: string;
  /**
   * The time left while the speed is not steady yet [kpadopt5-transfer]
   */
  transferEstimating: string;
  /**
   * The time left, "about 3 min left" [kpadopt5-transfer]
   */
  transferLeft: (duration: string) => string;
  /**
   * A transfer that moved no byte for a while, "stalled for 12 s" [kpadopt5-transfer]
   */
  transferStalled: (duration: string) => string;
  /**
   * A finished transfer's average speed, "avg 21 MB/s" [kpadopt5-transfer]
   */
  transferAverage: (rate: string) => string;
  /**
   * A finished transfer's duration, "in 3 min 12 s" [kpadopt5-transfer]
   */
  transferTook: (duration: string) => string;
  /**
   * The first words of a failed transfer's note [kpadopt5-transfer]
   */
  transferFailed: string;
  /**
   * The Retry button of a failed transfer [kpadopt5-transfer]
   */
  transferRetry: string;
  /**
   * What a screen reader hears at 25, 50, 75 and 100 % [kpadopt5-transfer]
   */
  transferAnnounce: (name: string, percent: number) => string;
  /**
   * The banner's first line when the connection is gone (js/connection.js) [kpadopt5-connection]
   */
  connOffline: string;
  /**
   * What is true meanwhile and what happens next [kpadopt5-connection]
   */
  connOfflineWhy: string;
  /**
   * The first line while a try is under way [kpadopt5-connection]
   */
  connReconnecting: string;
  /**
   * Its second line on the first try [kpadopt5-connection]
   */
  connReconnectingWhy: string;
  /**
   * Its second line from the second try, "Attempt 2" [kpadopt5-connection]
   */
  connAttempt: (n: number) => string;
  /**
   * The time to the next automatic try, "next try in 8 s" [kpadopt5-connection]
   */
  connNextTry: (duration: string) => string;
  /**
   * The exact count of changes waiting, "3 changes waiting" [kpadopt5-connection]
   */
  connPending: (n: number) => string;
  /**
   * The first line when the connection is back [kpadopt5-connection]
   */
  connBack: string;
  /**
   * Its second line with nothing waiting [kpadopt5-connection]
   */
  connBackWhy: string;
  /**
   * Its second line with changes waiting, "Sending 3 changes" [kpadopt5-connection]
   */
  connBackSending: (n: number) => string;
  /**
   * The first line when the server refuses while the browser is online [kpadopt5-connection]
   */
  connRefused: string;
  /**
   * Its second line when the app gives none [kpadopt5-connection]
   */
  connRefusedWhy: string;
  /**
   * The Retry now button [kpadopt5-connection]
   */
  connRetry: string;
  /**
   * A refresh choice, "Every 30 s" (js/auto-refresh.js) [kpadopt5-auto-refresh]
   */
  refreshEvery: (duration: string) => string;
  /**
   * The choice that stops the refreshing [kpadopt5-auto-refresh]
   */
  refreshPaused: string;
  /**
   * The entry that lets a person type any number of seconds [kpadopt5-auto-refresh]
   */
  refreshOther: string;
  /**
   * The accessible name of the refresh chooser [kpadopt5-auto-refresh]
   */
  refreshLabel: string;
  /**
   * The button that refreshes at once [kpadopt5-auto-refresh]
   */
  refreshNow: string;
  /**
   * The freshness line's words while the app reads [kpadopt5-auto-refresh]
   */
  refreshUpdating: string;
  /**
   * The note after a failed refresh, "Last refresh failed, retrying in 30 s" [kpadopt5-auto-refresh]
   */
  refreshFailed: (next: string) => string;
  /**
   * The note after a failed refresh while paused, when nothing retries by itself, "Last refresh failed." [live-options-1019]
   */
  refreshFailedPaused: string;
  /**
   * The button that retries a failed refresh at once [kpadopt5-auto-refresh]
   */
  refreshRetryNow: string;
  /**
   * The label of the field for any number of seconds [kpadopt5-auto-refresh]
   */
  refreshCustomLabel: string;
  /**
   * The button that sets that number [kpadopt5-auto-refresh]
   */
  refreshApply: string;
  /**
   * What a number that is not a whole number of seconds says [kpadopt5-auto-refresh]
   */
  refreshInvalid: string;
  /**
   * The detail pane's plate while nothing is picked (js/master-detail.js) [kpadopt5-master-detail]
   */
  paneEmpty: string;
  /**
   * The Back button at the top of a folded detail pane [kpadopt5-master-detail]
   */
  paneBack: string;
  /**
   * What the busy card over the detail pane says [kpadopt5-master-detail]
   */
  paneLoading: string;
  /**
   * The errored detail's first line, when the page gives none [kpadopt5-master-detail]
   */
  paneErrorTitle: string;
  /**
   * Its sentence, when the page gives none [kpadopt5-master-detail]
   */
  paneErrorText: string;
  /**
   * The Retry button of an errored detail [kpadopt5-master-detail]
   */
  paneRetry: string;
  busy: string;
  close: string;
  /**
   * The accessible name of a collapsed navigation's toggle
   */
  menu: string;
  /**
   * The same toggle once the navigation is open
   */
  closeMenu: string;
  /**
   * The accessible name of a mega-menu button in the bar that holds no words of its own [scope-48]
   */
  navDisclosure: string;
  /**
   * The accessible name of a hidden side navigation's toggle
   */
  sidebar: string;
  /**
   * The same toggle once that side navigation is open
   */
  closeSidebar: string;
  /**
   * The accessible name of an empty slim-rail toggle while the rail is expanded
   */
  collapseRail: string;
  /**
   * The same toggle while the rail is collapsed to its icons
   */
  expandRail: string;
  /**
   * The name of the resize handle on a resizable side navigation
   */
  resizeSidebar: string;
  /**
   * The control that returns the reader to the top of the page
   */
  backToTop: string;
  previous: string;
  next: string;
  finish: string;
  back: string;
  removeNamed: (name: string) => string;
  noResults: string;
  oneResult: string;
  manyResults: (n: number) => string;
  noCommands: string;
  oneCommand: string;
  manyCommands: (n: number) => string;
  commandPlaceholder: string;
  commandsLabel: string;
  shortcutsLabel: string;
  /**
   * The visible word on the command palette's trigger in the bar [scope-48]
   */
  paletteTrigger: string;
  /**
   * The hotkey as that trigger prints it: ⌘K on a Mac, Ctrl K elsewhere [scope-48]
   */
  paletteHotkey: (key: string, mac: boolean) => string;
  tableSearch: string;
  tableSearchLabel: string;
  tableSelectAll: string;
  tableSelectRow: (key: string) => string;
  tableEmpty: string;
  tableRows: (n: number) => string;
  tableRowsFiltered: (shown: number, total: number) => string;
  tablePage: (at: number, of: number) => string;
  tableRegion: string;
  /**
   * The name of the "In" choice beside the search box
   */
  tableSearchScope: string;
  /**
   * The "In" choice's first option
   */
  tableSearchAllColumns: string;
  /**
   * The filter panel's toggle, with the number of active filters
   */
  tableFilters: (active: number) => string;
  /**
   * The filter panel's group name
   */
  tableFiltersLabel: string;
  /**
   * The list of removable filter pills
   */
  tableActiveFilters: string;
  /**
   * A choice filter's pill
   */
  tableFilterValue: (column: string, value: string) => string;
  /**
   * A range or date filter's pill; an open end is `tableFilterOpenEnd`
   */
  tableFilterRange: (column: string, from: string, to: string) => string;
  /**
   * The open end of a range in its pill
   */
  tableFilterOpenEnd: string;
  /**
   * The lower bound's accessible name
   */
  tableFilterFrom: (column: string) => string;
  /**
   * The upper bound's accessible name
   */
  tableFilterTo: (column: string) => string;
  /**
   * A pill's remove button
   */
  tableRemoveFilter: (label: string) => string;
  tableClearFilters: string;
  /**
   * The word before the filters that are on, in a list toolbar's active row (js/toolbar.js) [kpadopt-controls]
   */
  toolbarShowingOnly: string;
  /**
   * The toolbar's button that turns every filter off [kpadopt-controls]
   */
  toolbarClearAll: string;
  /**
   * The accessible name of a line of keyboard shortcuts (js/keys.js) [kpadopt-controls]
   */
  keysLabel: string;
  /**
   * What a heat grid's or matrix's cell says while its value is being read (js/heat.js) [kpadopt2-grids]
   */
  heatLoading: string;
  /**
   * A heat grid or matrix with no rows [kpadopt2-grids]
   */
  heatEmpty: string;
  /**
   * A heat grid or matrix that could not be read [kpadopt2-grids]
   */
  heatFailed: string;
  /**
   * The less end of a heat legend's value ramp (js/heat.js, setHeatLegend) [gap-tab-count]
   */
  heatLegendLess: string;
  /**
   * The more end of the ramp [gap-tab-count]
   */
  heatLegendMore: string;
  /**
   * The name of a heat grid's legend for a screen reader [gap-tab-count]
   */
  heatLegendLabel: string;
  /**
   * The heat legend's key line: the arrow keys [gap-tab-count]
   */
  heatKeysMove: string;
  /**
   * Enter or Space on a cell: pick the column [gap-tab-count]
   */
  heatKeysPick: string;
  /**
   * Escape: drop the picked column [gap-tab-count]
   */
  heatKeysDrop: string;
  /**
   * Page Up and Page Down: show earlier or later columns [gap-tab-count]
   */
  heatKeysPage: string;
  /**
   * A matrix cell that pairs a row with itself, for a screen reader [kpadopt2-grids]
   */
  matrixSelf: string;
  /**
   * A matrix cell with no answer, for a screen reader [kpadopt2-grids]
   */
  matrixUnknown: string;
  /**
   * The way out of the no-match state
   */
  tableClearSearch: string;
  /**
   * The "+ Add filter" button of a table in the add-filter mode, with the number of active filters
   */
  tableAddFilter: (active: number) => string;
  /**
   * The name of its menu of filterable columns
   */
  tableAddFilterMenu: string;
  /**
   * A column in that menu, not filtered yet
   */
  tableAddFilterItem: (column: string) => string;
  /**
   * A column in that menu that is filtered already: choosing it edits that filter
   */
  tableAddFilterItemActive: (column: string) => string;
  /**
   * The mark beside a filtered column in the menu
   */
  tableFilterMarked: string;
  /**
   * The editor's accessible name
   */
  tableFilterEditor: (column: string) => string;
  /**
   * The editor's list of choices, for a screen reader
   */
  tableFilterChoicesLegend: (column: string) => string;
  /**
   * A bound's visible label in the editor
   */
  tableFilterBound: (
    kind: "range" | "date",
    bound: "from" | "to",
    column: string,
  ) => string;
  /**
   * A choice filter's one pill in the add-filter mode
   */
  tableFilterChoicePill: (column: string, values: string[]) => string;
  /**
   * A range or date filter's pill in the add-filter mode; an open end is left out
   */
  tableFilterSpanPill: (
    column: string,
    from: string,
    to: string,
    kind: "range" | "date",
  ) => string;
  /**
   * A pill's own button, which reopens its editor
   */
  tableEditFilter: (label: string) => string;
  tableFilterApply: string;
  tableFilterCancel: string;
  /**
   * The add-filter mode's way out of every filter, shown from two pills
   */
  tableFilterClearAll: string;
  /**
   * A number bound that is not a number
   */
  tableFilterNotNumber: (label: string, value: string) => string;
  /**
   * A date bound the picker cannot read
   */
  tableFilterNotDate: (label: string, value: string) => string;
  /**
   * A range whose lower bound is above its upper
   */
  tableFilterBackwards: (from: string, to: string) => string;
  tableDensity: string;
  tableDensityComfortable: string;
  tableDensityCompact: string;
  tableRowsPerPage: string;
  /**
   * The status line: "Showing 1–25 of 60"
   */
  tableShowing: (
    from: number,
    to: number,
    count: number,
    total: number,
  ) => string;
  /**
   * The action bar's count
   */
  tableSelected: (n: number) => string;
  tableClearSelection: string;
  /**
   * The card layout's sort control
   */
  tableSortBy: string;
  /**
   * Its first option
   */
  tableSortNone: string;
  tableSortAscending: string;
  tableSortDescending: string;
  tableFailed: string;
  tableRetry: string;
  /**
   * One key of a multi-column sort in words; `kind` is text, number, date or order
   */
  tableSortKey: (
    column: string,
    direction: "ascending" | "descending",
    kind: string,
  ) => string;
  /**
   * The multi-sort summary, from the keys in order
   */
  tableSortedBy: (keys: string[]) => string;
  tableNotSorted: string;
  tableSortReset: string;
  tableBusyElapsed: (seconds: number) => string;
  /**
   * The column menu's button
   */
  tableColumns: string;
  /**
   * The column menu's name
   */
  tableColumnsLabel: string;
  /**
   * A column that cannot be hidden, in the menu
   */
  tableColumnLocked: (column: string) => string;
  tableShowAllColumns: string;
  tableColumnsShown: (shown: number, total: number) => string;
  /**
   * The expansion column's header, read by a screen reader only
   */
  tableDetailsColumn: string;
  /**
   * A row's expand button
   */
  tableRowDetails: (key: string) => string;
  /**
   * A group row's fold button: the group's name and how many of its rows match [J2]
   */
  tableGroupRows: (group: string, count: number) => string;
  /**
   * An editable cell's button
   */
  tableEdit: (column: string, key: string, value: string) => string;
  /**
   * The editor's accessible name
   */
  tableEditField: (column: string, key: string) => string;
  /**
   * Said when an editor opens
   */
  tableEditing: (column: string, key: string) => string;
  tableEdited: (
    key: string,
    column: string,
    before: string,
    after: string,
  ) => string;
  tableEditUndone: (key: string, column: string, value: string) => string;
  tableEditCancelled: string;
  tableEditRequired: string;
  /**
   * A rejected edit with no message of its own
   */
  tableEditInvalid: string;
  /**
   * The keyboard grid's line before the first cell is focused
   */
  tableGridStart: string;
  /**
   * Row 0 is the header row
   */
  tableGridPosition: (
    row: number,
    rows: number,
    column: string,
    text: string,
  ) => string;
  formRequired: string;
  formInvalid: string;
  formSummaryOne: string;
  formSummaryMany: (n: number) => string;
  fieldFallbackName: string;
  /**
   * The word beside a switch that is on [gap-11]
   */
  switchOn: string;
  /**
   * The word beside a switch that is off [gap-11]
   */
  switchOff: string;
  calendarOpen: string;
  calendarButton: string;
  dateFormatHint: string;
  previousMonth: string;
  nextMonth: string;
  monthTitle: (month: string, year: number) => string;
  /**
   * The month title's name: it opens the twelve months [scope-89]
   */
  chooseMonth: (title: string) => string;
  /**
   * The year title's name: it opens twelve years [scope-89]
   */
  chooseYear: (year: number) => string;
  previousYear: string;
  nextYear: string;
  /**
   * Twelve years back, in the year grid
   */
  previousYears: string;
  /**
   * Twelve years on, in the year grid
   */
  nextYears: string;
  /**
   * The month grid's name, and what is said when it opens
   */
  monthGrid: (year: number) => string;
  /**
   * The year grid's name, and what is said when it opens
   */
  yearGrid: (from: number, to: number) => string;
  /**
   * The year grid's title
   */
  yearRange: (from: number, to: number) => string;
  weekdays: string[];
  months: string[];
  /**
   * The month grid's cells, where a full name does not fit [scope-89]
   */
  monthsShort: string[];
  dayLabel: (day: number, month: string, year: number) => string;
  uploadZone: string;
  uploadTooLarge: (size: string) => string;
  uploadTooMany: (max: number) => string;
  uploadTotalTooLarge: (max: string) => string;
  uploadWrongType: (accept: string) => string;
  uploadProgress: (name: string) => string;
  wizardStep: (at: number, of: number) => string;
  copy: string;
  copied: string;
  copyBlocked: string;
  copiedAnnouncement: (value: string) => string;
  copyBlockedAnnouncement: string;
  undo: string;
  deleted: string;
  /**
   * A secret field's name when the page gives none (js/patterns.js, `.kp-secret`) [kpadopt4]
   */
  secretLabel: string;
  /**
   * The button that shows a secret
   */
  secretReveal: string;
  /**
   * The same button while the secret is shown
   */
  secretHide: string;
  /**
   * The button of a secret shown only while it is held (`data-kp-secret-reveal-on="hold"`)
   */
  secretHold: string;
  /**
   * The button that copies a secret without showing it
   */
  secretCopy: string;
  /**
   * The words beside the spinner while a secret is fetched
   */
  secretReading: string;
  /**
   * The reveal button's words after a secret could not be read
   */
  secretRetry: string;
  /**
   * The countdown under a shown secret, "Hides in 28 s"
   */
  secretHidesIn: (seconds: number) => string;
  /**
   * The countdown's words while the reader selects or holds the focus in the value
   */
  secretPaused: string;
  /**
   * The line under a shown secret that hides only when asked (`data-kp-secret-ttl="0"`)
   */
  secretStaysShown: string;
  /**
   * What a screen reader hears when a secret is shown, "Shown. Hides in 30 s."
   */
  secretShown: (seconds: number) => string;
  /**
   * What it hears when the secret is hidden again
   */
  secretHidden: string;
  /**
   * The line under a secret that could not be read
   */
  secretFailed: (reason: string) => string;
  /**
   * The reason when a secret has no value, loader or listener to read it from
   */
  secretNoSource: string;
  splitLabel: string;
  reorderHandle: (name: string) => string;
  reorderMoved: (name: string, at: number, of: number) => string;
  tileFallbackName: string;
  tileLabel: (
    name: string,
    column: number,
    row: number,
    w: number,
    h: number,
  ) => string;
  contrastMissing: (token: string) => string;
  contrastReport: (ratio: string, token: string, verdict: string) => string;
  colourHue: string;
  colourSaturation: string;
  colourLightness: string;
  contrastPasses: string;
  contrastFails: string;
  confirm: string;
  confirmAccept: string;
  confirmCancel: string;
  confirmDescription: string;
  /**
   * The heading over what still uses the thing a confirmation would remove, "Still in use by 2" (`data-kp-confirm-dependents`) [kpadopt4]
   */
  confirmDependents: (count: number) => string;
  /**
   * What a confirmation that may not go ahead says when the page gives no sentence (`data-kp-confirm-blocked`) [kpadopt4]
   */
  confirmBlocked: string;
  /**
   * The one button of a confirmation that may not go ahead [kpadopt4]
   */
  confirmClose: string;
  /**
   * The button that closes an alarm that must be acknowledged, when the caller names none [scope-94]
   */
  alarmAction: string;
  /**
   * The button that stops an alarm's countdown and keeps it open [scope-94]
   */
  alarmKeepOpen: string;
  /**
   * The key hint beside the acknowledge button; hidden from a screen reader [scope-94]
   */
  alarmHint: string;
  /**
   * The whole seconds left under the countdown bar; hidden from a screen reader [scope-94]
   */
  alarmCountdown: (seconds: number) => string;
  /**
   * Said once when an alarm that closes by itself opens [scope-94]
   */
  alarmClosesBy: (seconds: number) => string;
  /**
   * Said when an alarm that must be acknowledged opens [scope-94]
   */
  alarmPressTo: (action: string) => string;
  /**
   * Said when Keep open stopped the countdown [scope-94]
   */
  alarmKeptOpen: (action: string) => string;
  save: string;
  mainNavigation: string;
  skipToContent: string;
  /**
   * The diagnostics row that lists unknown hook values [AR44]
   */
  diagnosticsEffects: string;
  diagnosticsEffectsNone: string;
  /**
   * The stamp a register may print on an emphasis reveal (the dossier) [AR35]
   */
  classified: string;
  /**
   * The neutral boot line of an arrival, for a theme that performs one and has no words of its own in `arrivalWordsByTheme` or `arrivalLinesByTheme` [SW2, scope-84]
   */
  arrivalLine: string;
  /**
   * A theme's own words for the counting boot, keyed by theme [scope-84]: its line, the word before the percentage, and the word that closes it. A word the entry leaves out is the neutral one.
   */
  arrivalWordsByTheme: Record<
    string,
    {
      line?: string;
      progress?: string;
      ready?: string;
    }
  >;
  /**
   * The lines a theme's own boot shows instead of that one line, in order, keyed by theme [S49, A11]. A `{count}` in a line is replaced by a number counting up to `--kp-arrival-count` (640 by default), which is how retro's memory test reads.
   */
  arrivalLinesByTheme: Record<string, string[]>;
  /**
   * The neutral word before the percentage on that line
   */
  arrivalProgress: string;
  /**
   * The neutral word that closes the boot line
   */
  arrivalReady: string;
  /**
   * The button that ends the arrival at once
   */
  arrivalSkip: string;
  /**
   * A live dimension label before the first measurement lands (blueprint) [S48]
   */
  measureLoading: string;
  /**
   * The size of the box the measurement frame holds (blueprint) [scope-18]
   */
  measureBox: (w: number, h: number) => string;
  /**
   * A time chart's plot, for a screen reader: what it is and its keys [scope-143]
   */
  chartPlot: (label: string) => string;
  /**
   * A chart's 24-hour spark line, for a screen reader [scope-143]
   */
  chartSpark: (label: string) => string;
  /**
   * The name of a chart's legend [scope-143]
   */
  chartSources: (label: string) => string;
  /**
   * A legend button's title: what hovering and clicking it do [scope-143]
   */
  chartSource: (label: string) => string;
  /**
   * The legend's way back to every source [scope-143]
   */
  chartShowAll: string;
  /**
   * Its title [scope-143]
   */
  chartShowAllTitle: string;
  /**
   * The hint under a legend, for a pointer [scope-143]
   */
  chartHintPointer: string;
  /**
   * The same hint, for a finger [scope-143]
   */
  chartHintTouch: string;
  /**
   * The word in a pinned tooltip's head [scope-143]
   */
  chartPinned: string;
  /**
   * The pinned tooltip's ✕, named [scope-143]
   */
  chartRelease: string;
  /**
   * The tooltip's foot: what the ▲/▼ column is [scope-143]
   */
  chartChange: string;
  /**
   * An event's link in a pinned tooltip [scope-143]
   */
  chartOpen: string;
  /**
   * An event marker's title [scope-143]
   */
  chartMark: (label: string, time: string) => string;
  /**
   * The zoom chip, before its Reset, from its two ends; used only when a consumer overrides it and not `chartZoomedSpan` [scope-143]
   */
  chartZoomed: (from: string, to: string) => string;
  /**
   * The zoom chip, before its Reset: `span` is the zoom as the chart's `range` time style prints it [scope-143]
   */
  chartZoomedSpan: (span: string) => string;
  /**
   * The zoom chip's button [scope-143]
   */
  chartReset: string;
  /**
   * Its title [scope-143]
   */
  chartResetTitle: string;
  /**
   * A rise, as the readout says it [scope-143]
   */
  chartUp: string;
  /**
   * A fall, as the readout says it [scope-143]
   */
  chartDown: string;
  /**
   * No change, as the readout says it [scope-143]
   */
  chartSame: string;
  /**
   * A spark line's value when no time is under the crosshair [scope-143]
   */
  chartNow: string;
  /**
   * A time chart with no readings in its window, in the plot at its height [scope-143]
   */
  chartEmpty: string;
  /**
   * The Retry under a failed chart's sentence [t56-statefix-1018]
   */
  chartRetry: string;
  /**
   * The failed plate's heading of a key figure's trend when the page gives no sentence [kyu-plates-1019]
   */
  chartTrendFailed: string;
  /**
   * The failed plate's heading of a progress bar when the page gives no title [kyu-plates-1019]
   */
  progressFailed: string;
  /**
   * Under a chart whose every source has one reading (with `onePointNote`) [scope-143]
   */
  chartOnePoint: string;
  /**
   * A loading chart (`data-kp-chart-loading`), for a screen reader [scope-143]
   */
  chartLoading: string;
  /**
   * A pinned tooltip's head once a live update moved the window past the pin [scope-143]
   */
  chartPinnedOutside: string;
  /**
   * A menu button's menu while its entries load: one disabled entry (js/menu-button.js) [scope-143]
   */
  menuLoading: string;
  /**
   * The title of a menu button with no entries, which stays and does nothing (`data-kp-menu-empty="disable"`) [scope-143]
   */
  menuEmpty: string;
  /**
   * The word on the button a folding bar puts its overflowing links behind (`data-kp-nav-fold`, js/nav-fold.js) [kpadopt-nav]
   */
  navMore: string;
  /**
   * The name of that button's menu, for a screen reader [kpadopt-nav]
   */
  navMoreMenu: string;
  /**
   * Said after the button's name when a link behind it carries a counter [kpadopt-nav]
   */
  navMoreAlert: string;
  /**
   * Under a folded link that cannot be followed now (`aria-disabled`, `disabled`), as its reason [kpadopt-nav]
   */
  navMoreUnavailable: string;
  /**
   * The tour card's button to the next step (js/tour.js) [scope-143]
   */
  tourNext: string;
  /**
   * Its title [scope-143]
   */
  tourNextTitle: string;
  /**
   * The tour card's button to the step before [scope-143]
   */
  tourBack: string;
  /**
   * Its title [scope-143]
   */
  tourBackTitle: string;
  /**
   * The next button on the last step, which ends the tour [scope-143]
   */
  tourDone: string;
  /**
   * Its title [scope-143]
   */
  tourDoneTitle: string;
  /**
   * The tour card's button that ends the tour at once [scope-143]
   */
  tourSkip: string;
  /**
   * Its title [scope-143]
   */
  tourSkipTitle: string;
  /**
   * The tour card's count, "1 of 5"; `of` counts only the steps whose part is on the page [scope-143]
   */
  tourCount: (n: number, of: number) => string;
  /**
   * A key figure's trend axis: the word after the first point's clock when it is today ("07:00 today") [scope-143]
   */
  chartToday: string;
  /**
   * The same when it is yesterday ("14:40 yesterday") [scope-143]
   */
  chartYesterday: string;
  /**
   * A key figure's trend, for a screen reader: its keys [scope-143]
   */
  chartTrendKeys: string;
  /**
   * A meter's share, in the words a screen reader hears, when the page names it nothing else ("62% used") [scope-143]
   */
  meterUsed: string;
  /**
   * A meter with no share, for a screen reader [scope-143]
   */
  meterNotMeasured: string;
  /**
   * A loading meter, for a screen reader [scope-143]
   */
  meterMeasuring: string;
  /**
   * The calendar's busy text when the page gives none (`Loading…`)
   */
  loading?: string;
  /**
   * The word a busy picture spells on screen, deciphered letter by letter (cyberpunk's skeletons, busy bars and meters); `setStrings()` hands it to the stylesheet [scope-143]
   */
  loadingWord: string;
  /**
   * The name of a month heatmap's row of month buttons (js/calendar.js) [scope-143]
   */
  calendarNav: string;
  /**
   * A month heatmap's button to the month before; its name and title are `previousMonth` [scope-143]
   */
  calendarPrev: string;
  /**
   * Its button to the month after; its name and title are `nextMonth` [scope-143]
   */
  calendarNext: string;
  /**
   * Its button back to today's month, with today picked [scope-143]
   */
  calendarToday: string;
  /**
   * That button's title [scope-143]
   */
  calendarTodayTitle: string;
  /**
   * A day's name and title: the date (dd/mm/yyyy) and what the page says about it [scope-143]
   */
  calendarDay: (date: string, label: string) => string;
  /**
   * What a day still to come is, when the page says nothing about it [scope-143]
   */
  calendarFuture: string;
  /**
   * What every day is while the calendar loads [scope-143]
   */
  calendarLoading: string;
  /**
   * What a past day is when the page says nothing about it [scope-143]
   */
  calendarUnknown: string;
  /**
   * The name of a week agenda when the page gives none (js/week.js)
   */
  weekGroup: string;
  /**
   * The week agenda's button to the week before
   */
  weekPrev: string;
  /**
   * Its button to the week after
   */
  weekNext: string;
  /**
   * Its button back to the current week; also the word under today's date
   */
  weekToday: string;
  /**
   * The now-line's name
   */
  weekNow: string;
  /**
   * The now-line's name with the time, "now, 14:32"
   */
  weekNowAt: (time: string) => string;
  /**
   * The overflow pill's text, "+3"
   */
  weekMore: (n: number) => string;
  /**
   * The overflow pill's name, "3 more runs on Tue 17/10"
   */
  weekMoreLabel: (n: number, day: string) => string;
  /**
   * What a next run is called in a pill's name
   */
  weekNextRun: string;
  /**
   * What a switched-off item is called in a pill's name
   */
  weekOff: string;
  /**
   * What a skipped item is called in a pill's name
   */
  weekSkipped: string;
  /**
   * What a reference item (the host's own round) is called in a pill's name
   */
  weekReference: string;
  /**
   * The sentence of a week with nothing in it
   */
  weekEmpty: string;
  /**
   * The sentence of a week being read
   */
  weekLoading: string;
  /**
   * The name of a run timeline when the page gives none (js/gantt.js)
   */
  ganttGroup: string;
  /**
   * The name of the range buttons
   */
  ganttRange: string;
  /**
   * A range button's title, "Show the last 6 hours"
   */
  ganttRangeTitle: (label: string) => string;
  /**
   * The now-line's name
   */
  ganttNow: string;
  /**
   * The selected range's chip label
   */
  ganttSelection: string;
  /**
   * The button that lifts the selected range
   */
  ganttClear: string;
  /**
   * Its title
   */
  ganttClearTitle: string;
  /**
   * The chip's text, "Selected 14:00 to 16:30"
   */
  ganttSelected: (from: string, to: string) => string;
  /**
   * The sentence of a timeline with no run in the window
   */
  ganttEmpty: string;
  /**
   * The sentence of a timeline being read
   */
  ganttLoading: string;
  /**
   * A queued run's state word
   */
  ganttQueued: string;
  /**
   * A running run's state word
   */
  ganttRunning: string;
  /**
   * A finished run's state word
   */
  ganttOk: string;
  /**
   * A failed run's state word
   */
  ganttFailed: string;
  /**
   * What the timeline's status region says when a run changes state, "Backup: failed"
   */
  ganttAnnounce: (label: string, state: string) => string;
  /**
   * A bar's name, "Backup, 02:00 to 02:14, failed"
   */
  ganttBar: (label: string, when: string, state: string) => string;
  /**
   * The button that makes the window follow now
   */
  ganttLive: string;
  /**
   * Its title
   */
  ganttLiveTitle: string;
  /**
   * The button that makes the selected range the window
   */
  ganttZoom: string;
  /**
   * Its title
   */
  ganttZoomTitle: string;
  /**
   * A length of time as a person reads it, "14 min", "1 h 5 min", "45 s"
   */
  ganttDuration: (seconds: number) => string;
  /**
   * The one-line hint under the timeline on how to read and use it
   */
  ganttHint: string;
  /**
   * The same hint of a timeline without its range toolbar (no brush): only opening a bar [gaps-time-1019]
   */
  ganttHintOpen: string;
  /**
   * The range button that fits the window to everything on the timeline (`all` in `data-kp-gantt-ranges`) [kpadopt4]
   */
  ganttAll: string;
  /**
   * Its title
   */
  ganttAllTitle: string;
  /**
   * A point event's name, "Applied, 02/10/2026" [kpadopt4]
   */
  ganttPoint: (label: string, when: string) => string;
  /**
   * The word after a planned point's name, still to come
   */
  ganttPlanned: string;
  /**
   * The name of marks too close to tell apart, "3 events: Applied, Mail, Call"
   */
  ganttCluster: (count: number, labels: string) => string;
  /**
   * The count on such a mark, "+2"
   */
  ganttClusterMore: (more: number) => string;
  /**
   * The hint under a timeline of point events
   */
  ganttPointsHint: string;
  /**
   * The words on a pipeline's loading card (js/pipeline.js) [kpadopt4]
   */
  pipelineLoading: string;
  /**
   * The sentence over a pipeline with no cards, when the page gives none
   */
  pipelineEmpty: string;
  /**
   * The sentence over a pipeline that could not be read
   */
  pipelineFailed: (reason: string) => string;
  /**
   * Its Try again button
   */
  pipelineRetry: string;
  /**
   * The word on a failing lane's name plate (`danger: true`), so the colour is never the only carrier [r1r2-1019]
   */
  pipelineDanger: string;
  /**
   * The word in a lane with no cards
   */
  pipelineNone: string;
  /**
   * The button under a lane's first cards, "+4 more"
   */
  pipelineMore: (more: number) => string;
  /**
   * The same button while the lane shows every card
   */
  pipelineLess: string;
  /**
   * A lane's share of all cards, "25 % of all"
   */
  pipelineShare: (percent: number) => string;
  /**
   * The uptime strip's busy card and its summary while the history is read (js/uptime.js) [kpadopt4-ideas]
   */
  uptimeLoading: string;
  /**
   * Its summary when no period has data yet [kpadopt4-ideas]
   */
  uptimeEmpty: string;
  /**
   * Its summary when the history could not be read and the page gave no sentence [kpadopt4-ideas]
   */
  uptimeError: string;
  /**
   * The window's share up, from "99.4" to "99.4%" [kpadopt4-ideas]
   */
  uptimePercent: (percent: string) => string;
  /**
   * The window's incident count, exact: "1 incident", "12 incidents" [kpadopt4-ideas]
   */
  uptimeIncidents: (n: number) => string;
  /**
   * The summary as one sentence, for a screen reader [kpadopt4-ideas]
   */
  uptimeSummary: (percent: string, incidents: number) => string;
  /**
   * The strip's name: its title and its summary [kpadopt4-ideas]
   */
  uptimeTrack: (label: string, summary: string) => string;
  /**
   * A period's state as a word: up, degraded, down, no data [kpadopt4-ideas]
   */
  uptimeStateWord: (state: string) => string;
  /**
   * A cell's name and hover card: its period, its state and its figures [kpadopt4-ideas]
   */
  uptimeCell: (
    label: string,
    state: string,
    percent: string | null,
    incidents: number | null,
  ) => string;
  /**
   * Under a strip too narrow for every cell: "55 of 90 shown" [kpadopt4-ideas]
   */
  uptimeShown: (shown: number, total: number) => string;
  /**
   * A period's figures in its quick info: "99.4% up · 1 incident" [kpadopt4 r2]
   */
  uptimeInfoFigures: (
    percent: string | null,
    incidents: number | null,
  ) => string;
  /**
   * The link in a period's quick info to the period's own page: "Open 14/10/2026" [kpadopt4 r2]
   */
  uptimeOpen: (label: string) => string;
  /**
   * The typed confirmation's label; the name is set in as code where this sentence puts it (js/components.js) [kpadopt4-ideas]
   */
  typeConfirmLabel: (name: string) => string;
  /**
   * Its rule when capitals count [kpadopt4-ideas]
   */
  typeConfirmRuleExact: string;
  /**
   * Its rule when capitals do not count (`data-kp-confirm-case="any"`) [kpadopt4-ideas]
   */
  typeConfirmRuleAnyCase: string;
  /**
   * The badge once the typed name matches [kpadopt4-ideas]
   */
  typeConfirmMatched: string;
  /**
   * The line while what is typed does not match yet [kpadopt4-ideas]
   */
  typeConfirmMismatch: string;
  /**
   * The heading over what the action will delete [kpadopt4-ideas]
   */
  typeConfirmLoses: string;
  /**
   * A command block's terminal by its key: WSL, PowerShell, cmd, Garuda, bash; another key as written (js/command.js) [kpadopt4-ideas]
   */
  commandShell: (shell: string) => string;
  /**
   * The badge of a Windows command that needs a terminal opened as administrator [kpadopt4-ideas]
   */
  commandAdmin: string;
  /**
   * Its tooltip [kpadopt4-ideas]
   */
  commandAdminTitle: string;
  /**
   * The badge of a Linux command that runs with root rights [kpadopt4-ideas]
   */
  commandSudo: string;
  /**
   * Its tooltip [kpadopt4-ideas]
   */
  commandSudoTitle: string;
  /**
   * The note under a command that says where it runs [kpadopt4-ideas]
   */
  commandRunsOn: (where: string) => string;
  /**
   * A timer's Start button (js/countdown.js, js/timer-disc.js) [kpadopt4-ideas]
   */
  timerStart: string;
  /**
   * The same button while it runs [kpadopt4-ideas]
   */
  timerPause: string;
  /**
   * The same button while it is paused [kpadopt4-ideas]
   */
  timerResume: string;
  /**
   * A timer's Reset button [kpadopt4-ideas]
   */
  timerReset: string;
  /**
   * The countdown's button that adds time: "+1 min" [kpadopt4-ideas]
   */
  timerAddMinute: (seconds: number) => string;
  /**
   * Its tooltip: "Add 1 min" [kpadopt4-ideas]
   */
  timerAddMinuteTitle: (seconds: number) => string;
  /**
   * The line under a paused timer, and what a screen reader hears [kpadopt4-ideas]
   */
  timerPaused: string;
  /**
   * The line under a timer whose time is up [kpadopt4-ideas]
   */
  timerEnded: string;
  /**
   * What a screen reader hears when a named timer ends [kpadopt4-ideas]
   */
  timerEndedNamed: (name: string) => string;
  /**
   * What a screen reader hears when a timer starts [kpadopt4-ideas]
   */
  timerStarted: string;
  /**
   * What a screen reader hears when it goes on after a pause [kpadopt4-ideas]
   */
  timerResumed: string;
  /**
   * The countdown's time left for a screen reader: "25 min left" [kpadopt4-ideas]
   */
  timerLeft: (duration: string) => string;
  /**
   * A timer's length in words: "25 min", "1 h 30 min", "45 s" [kpadopt4-ideas]
   */
  timerDuration: (seconds: number) => string;
  /**
   * The visual timer's face for a screen reader: "12 min left of 25 min" [kpadopt4-ideas]
   */
  timerDiscLeft: (left: string, total: string) => string;
  /**
   * The time left on the face in its last minute [kpadopt4-ideas]
   */
  timerUnderMinute: string;
  /**
   * The still line under the disc: "Focus · 25 min" [kpadopt4-ideas]
   */
  timerDiscCaption: (name: string, duration: string) => string;
  /**
   * The still line of a timer that counts down to a moment: "until 14:30" [gaps-time-1019]
   */
  timerUntil: (when: string) => string;
  /**
   * The range picker's preset group, for a screen reader (js/range.js) [kpadopt4-ideas]
   */
  rangePresets: string;
  /**
   * A preset in words: "Last 24 hours" [kpadopt4-ideas]
   */
  rangePresetName: (key: string) => string;
  /**
   * A preset on its segment: "24h" [kpadopt4-ideas]
   */
  rangePresetShort: (key: string) => string;
  /**
   * The button that opens the custom range [kpadopt4-ideas]
   */
  rangeCustom: string;
  /**
   * The custom panel's title [kpadopt4-ideas]
   */
  rangeCustomTitle: string;
  /**
   * A custom window's name in the line under the picker [kpadopt4-ideas]
   */
  rangeCustomName: string;
  /**
   * The start's label [kpadopt4-ideas]
   */
  rangeFrom: string;
  /**
   * The end's label [kpadopt4-ideas]
   */
  rangeTo: string;
  /**
   * A date field's name: "From, date (dd/mm/yyyy)" [kpadopt4-ideas]
   */
  rangeDateLabel: (bound: string) => string;
  /**
   * A time field's name [kpadopt4-ideas]
   */
  rangeTimeLabel: (bound: string) => string;
  /**
   * The time field's placeholder [kpadopt4-ideas]
   */
  rangeTimeHint: string;
  /**
   * The custom panel's Apply [kpadopt4-ideas]
   */
  rangeApply: string;
  /**
   * The custom panel's Cancel [kpadopt4-ideas]
   */
  rangeCancel: string;
  /**
   * The line under the picker: "Last 24 hours · 15/10/2026 14:05 – 16/10/2026 14:05" [kpadopt4-ideas]
   */
  rangeLabel: (name: string, from: string, to: string) => string;
  /**
   * A date that is not a whole dd/mm/yyyy [kpadopt4-ideas]
   */
  rangeErrorDate: (bound: string) => string;
  /**
   * A time that is not a time of day [kpadopt4-ideas]
   */
  rangeErrorTime: (bound: string) => string;
  /**
   * A window whose end does not come after its start [kpadopt4-ideas]
   */
  rangeErrorOrder: string;
  /**
   * A window that ends in the future [kpadopt4-ideas]
   */
  rangeErrorFuture: string;
  /**
   * A window longer than the picker allows [kpadopt4-ideas]
   */
  rangeErrorSpan: (days: number) => string;
  /**
   * The weekday picker's name when the page gives none (js/weekdays.js) [kpadopt5-time]
   */
  weekdaysGroup: string;
  /**
   * The short day names, Monday first: "Mon" [kpadopt5-time]
   */
  weekdaysShort: string[];
  /**
   * The one-letter day names, Monday first: "M" [kpadopt5-time]
   */
  weekdaysNarrow: string[];
  /**
   * The full day names, Monday first, a chip's hint and name [kpadopt5-time]
   */
  weekdaysLong: string[];
  /**
   * The month picker's chip for the last day [kpadopt5-time]
   */
  weekdaysLast: string;
  /**
   * The last day of the month in words [kpadopt5-time]
   */
  weekdaysLastDay: string;
  /**
   * The word that joins the last two days of a list: "Mon and Thu" [kpadopt5-time]
   */
  weekdaysAnd: string;
  /**
   * The quick picks' group name for a screen reader [kpadopt5-time]
   */
  weekdaysPresets: string;
  /**
   * The quick pick for Monday to Friday [kpadopt5-time]
   */
  weekdaysPresetWeekdays: string;
  /**
   * The quick pick for Saturday and Sunday [kpadopt5-time]
   */
  weekdaysPresetWeekend: string;
  /**
   * The quick pick for all seven days [kpadopt5-time]
   */
  weekdaysPresetEvery: string;
  /**
   * The line while no day is picked [kpadopt5-time]
   */
  weekdaysNone: string;
  /**
   * The line for all seven days [kpadopt5-time]
   */
  weekdaysEvery: string;
  /**
   * The line for Monday to Friday [kpadopt5-time]
   */
  weekdaysWeekdays: string;
  /**
   * The line for Saturday and Sunday [kpadopt5-time]
   */
  weekdaysWeekend: string;
  /**
   * A day of the month in words: "1st", "22nd" [kpadopt5-time]
   */
  weekdaysOrdinal: (n: number) => string;
  /**
   * The days of a month in a sentence: "On the 1st and 15th" [kpadopt5-time]
   */
  weekdaysOnThe: (list: string) => string;
  /**
   * What a picker below its least says: "Pick at least one day" [kpadopt5-time]
   */
  weekdaysMin: (n: number) => string;
  /**
   * The heading over a next-times list (js/runs.js) [kpadopt5-time]
   */
  runsTitle: string;
  /**
   * A run that is due now, where "in 2 days" would stand [kpadopt5-time]
   */
  runsNow: string;
  /**
   * How far away a run is: "in 2 days" [kpadopt5-time]
   */
  runsIn: (distance: string) => string;
  /**
   * The mark on a run after the clock was turned, positive hours forward: "Clock goes back 1 h" [kpadopt5-time]
   */
  runsClockChange: (hours: number) => string;
  /**
   * The button that shows the times past the count: "Show 10 more" [kpadopt5-time]
   */
  runsMore: (n: number) => string;
  /**
   * The same button once they are shown [kpadopt5-time]
   */
  runsFewer: string;
  /**
   * An empty list's sentence when the page gives none [kpadopt5-time]
   */
  runsEmpty: string;
  /**
   * A failed list's first line [kpadopt5-time]
   */
  runsFailed: string;
  /**
   * The failed list's button [kpadopt5-time]
   */
  runsRetry: string;
  /**
   * What the busy card says while the times are worked out [kpadopt5-time]
   */
  runsLoading: string;
  /**
   * What the status region says when the first run changes [kpadopt5-time]
   */
  runsNext: (moment: string) => string;
  /**
   * The snooze picker's button while nothing is snoozed (js/snooze.js) [kpadopt5-time]
   */
  snoozeTrigger: string;
  /**
   * The same button while a snooze is in force [kpadopt5-time]
   */
  snoozeChange: string;
  /**
   * The chip row's name for a screen reader [kpadopt5-time]
   */
  snoozeChoicesLabel: string;
  /**
   * A length as a choice: "In 4 hours" [kpadopt5-time]
   */
  snoozeIn: (key: string) => string;
  /**
   * "This evening 18:00" [kpadopt5-time]
   */
  snoozeTonight: (time: string) => string;
  /**
   * "Tomorrow 09:00" [kpadopt5-time]
   */
  snoozeTomorrow: (time: string) => string;
  /**
   * "Monday 09:00" [kpadopt5-time]
   */
  snoozeMonday: (time: string) => string;
  /**
   * "Next week 09:00" [kpadopt5-time]
   */
  snoozeNextWeek: (time: string) => string;
  /**
   * The last custom moment, offered at the top [kpadopt5-time]
   */
  snoozeLast: string;
  /**
   * The choice that opens the date and time panel [kpadopt5-time]
   */
  snoozePick: string;
  /**
   * What stands under that choice [kpadopt5-time]
   */
  snoozePickHint: string;
  /**
   * The panel's title [kpadopt5-time]
   */
  snoozePickTitle: string;
  /**
   * The panel's date field name [kpadopt5-time]
   */
  snoozeDateLabel: string;
  /**
   * The panel's time field name [kpadopt5-time]
   */
  snoozeTimeLabel: string;
  /**
   * The panel's confirm button [kpadopt5-time]
   */
  snoozeApply: string;
  /**
   * The panel's cancel button [kpadopt5-time]
   */
  snoozeCancel: string;
  /**
   * The line while snoozed: "Snoozed until Mon 20/10/2026 09:00 · in 2 days" [kpadopt5-time]
   */
  snoozeUntil: (moment: string) => string;
  /**
   * The line while nothing is snoozed [kpadopt5-time]
   */
  snoozeNone: string;
  /**
   * The button that ends a snooze [kpadopt5-time]
   */
  snoozeEnd: string;
  /**
   * Why the choices are off while the page saves [kpadopt5-time]
   */
  snoozeSaving: string;
  /**
   * A date that is not a whole dd/mm/yyyy [kpadopt5-time]
   */
  snoozeErrorDate: string;
  /**
   * A time that is not a time of day [kpadopt5-time]
   */
  snoozeErrorTime: string;
  /**
   * A moment that has passed [kpadopt5-time]
   */
  snoozeErrorPast: string;
  /**
   * A moment past the longest a reminder may be put off [kpadopt5-time]
   */
  snoozeErrorMax: (days: number) => string;
  /**
   * A dated option picker's name when the page gives none (js/dated-options.js) [kpadopt5-time]
   */
  datedGroup: string;
  /**
   * The badge on the newest option [kpadopt5-time]
   */
  datedLatest: string;
  /**
   * An option made under a minute ago [kpadopt5-time]
   */
  datedJustNow: string;
  /**
   * How long ago an option was made: "3 days ago" [kpadopt5-time]
   */
  datedAgo: (distance: string) => string;
  /**
   * The search field's placeholder, for a long list [kpadopt5-time]
   */
  datedSearch: string;
  /**
   * What the status region says when an option is chosen [kpadopt5-time]
   */
  datedChosen: (name: string) => string;
  /**
   * An empty picker's sentence when the page gives none [kpadopt5-time]
   */
  datedEmpty: string;
  /**
   * A failed picker's first line [kpadopt5-time]
   */
  datedFailed: string;
  /**
   * The failed picker's button [kpadopt5-time]
   */
  datedRetry: string;
  /**
   * What the busy card says while the options load [kpadopt5-time]
   */
  datedLoading: string;
  /**
   * What a sentence's summary says for a field with nothing in it: "(At)" (js/sentence.js) [kpadopt5-time]
   */
  sentenceEmpty: (label: string) => string;
  /**
   * The line under a sentence that names the field that is wrong: "At: Type a time of day." [kpadopt5-time]
   */
  sentenceWhy: (field: string, why: string) => string;
  /**
   * The name of a control that has none and no words before it [kpadopt5-time]
   */
  sentenceField: string;
  /**
   * A slider's name when the page gives none (js/slider.js) [kpadopt5-time]
   */
  sliderGroup: string;
  /**
   * The lower thumb's and box's name: "Temperature, minimum" [kpadopt5-time]
   */
  sliderMin: (label: string) => string;
  /**
   * The upper thumb's and box's name: "Temperature, maximum" [kpadopt5-time]
   */
  sliderMax: (label: string) => string;
  /**
   * The value box's name: "Temperature, value" [kpadopt5-time]
   */
  sliderValue: (label: string) => string;
  /**
   * The line under a slider for a typed value that is not a number in range [kpadopt5-time]
   */
  sliderInvalid: (min: string, max: string, unit: string) => string;
  /**
   * The line for a typed value that would bring the two thumbs too close [kpadopt5-time]
   */
  sliderGap: (gap: string, unit: string) => string;
  /**
   * The counter of a growing text box: "1 204 characters" (js/autosize.js) [kpadopt6-media]
   */
  autosizeChars: (n: number, text: string) => string;
  /**
   * Its line count: "12 lines" [kpadopt6-media]
   */
  autosizeLines: (n: number, text: string) => string;
  /**
   * Both counts in one line [kpadopt6-media]
   */
  autosizeBoth: (
    chars: number,
    charsText: string,
    lines: number,
    linesText: string,
  ) => string;
  /**
   * A count against the limit: "1 204 of 4 000 characters" [kpadopt6-media]
   */
  autosizeOf: (text: string, maxText: string) => string;
  /**
   * Said politely when the limit is reached [kpadopt6-media]
   */
  autosizeFull: (maxText: string) => string;
  /**
   * Said politely when 90 % of the limit is used [kpadopt6-media]
   */
  autosizeNear: (charsText: string, maxText: string) => string;
  /**
   * The sheet's words for Ctrl+Enter submitting the form [kpadopt6-media]
   */
  autosizeSubmit: string;
  /**
   * The sheet's heading for that key [kpadopt6-media]
   */
  autosizeGroup: string;
  /**
   * The busy card's words while a box waits for its text [kpadopt6-media]
   */
  autosizeLoading: string;
  /**
   * The line under a box whose text could not be saved, when the page gives none [kpadopt6-media]
   */
  autosizeFailed: string;
  /**
   * The accessible name of a link mark: "Copy link to Dead letter 4f2a" (js/anchors.js) [kpadopt6-media]
   */
  anchorCopyLink: (name: string) => string;
  /**
   * The word on the mark after a copy [kpadopt6-media]
   */
  anchorCopied: string;
  /**
   * The word on the mark when the clipboard refused [kpadopt6-media]
   */
  anchorFailed: string;
  /**
   * Said politely after a copy [kpadopt6-media]
   */
  anchorCopiedAnnounce: (name: string) => string;
  /**
   * Said politely when the clipboard refused [kpadopt6-media]
   */
  anchorFailedAnnounce: string;
  /**
   * The notice for an address that points at nothing here any more [kpadopt6-media]
   */
  anchorMissing: string;
  /**
   * The notice's button that clears the address [kpadopt6-media]
   */
  anchorShowAll: string;
  /**
   * The sheet's words for y then l on a row [kpadopt6-media]
   */
  anchorKeyCopy: string;
  /**
   * The sheet's heading for the anchor keys [kpadopt6-media]
   */
  anchorKeyGroup: string;
  /**
   * What a list that is still loading prints instead of a blank sheet (js/print.js) [kpadopt6-media]
   */
  printLoading: string;
  /**
   * The foot of a printed sheet: "Printed 17/10/2026 14:05" [kpadopt6-media]
   */
  printStamp: (moment: string) => string;
  /**
   * The viewer's "Open in a new tab" button (js/viewer.js) [kpadopt6-media]
   */
  viewerOpenNew: string;
  /**
   * The viewer's Download button [kpadopt6-media]
   */
  viewerDownload: string;
  /**
   * The viewer's Print button [kpadopt6-media]
   */
  viewerPrint: string;
  /**
   * The viewer's Copy link button [kpadopt6-media]
   */
  viewerCopyLink: string;
  /**
   * The reason a viewer button is off when there is no file [kpadopt6-media]
   */
  viewerNoFile: string;
  /**
   * The busy card's words while a file opens [kpadopt6-media]
   */
  viewerLoading: string;
  /**
   * The failed stage's heading [kpadopt6-media]
   */
  viewerFailedTitle: string;
  /**
   * The failed stage's sentence when the server gave none [kpadopt6-media]
   */
  viewerFailedText: string;
  /**
   * The heading of the stage that cannot show this kind of file here [kpadopt6-media]
   */
  viewerUnsupportedTitle: string;
  /**
   * Its sentence [kpadopt6-media]
   */
  viewerUnsupportedText: string;
  /**
   * The failed stage's Retry [kpadopt6-media]
   */
  viewerRetry: string;
  /**
   * The exact position in a set: "4 of 37" [kpadopt6-media]
   */
  viewerPosition: (index: number, total: number) => string;
  /**
   * The sheet's heading for the preview keys (js/docpreview.js) [kpadopt6-media]
   */
  previewKeyGroup: string;
  /**
   * The sheet's words for o [kpadopt6-media]
   */
  previewKeyOpen: string;
  /**
   * The sheet's words for d [kpadopt6-media]
   */
  previewKeyDownload: string;
  /**
   * The sheet's words for p [kpadopt6-media]
   */
  previewKeyPrint: string;
  /**
   * The sheet's words for Left in a group of documents [kpadopt6-media]
   */
  previewKeyPrevious: string;
  /**
   * The sheet's words for Right in a group of documents [kpadopt6-media]
   */
  previewKeyNext: string;
  /**
   * The sentence when a file takes too long to open [kpadopt6-media]
   */
  previewTimeout: string;
  /**
   * The sentence when the server says the file is gone [kpadopt6-media]
   */
  previewGone: string;
  /**
   * The sentence when the server answered with an error [kpadopt6-media]
   */
  previewServer: (status: number) => string;
  /**
   * The sentence when this browser cannot show a PDF here [kpadopt6-media]
   */
  previewNoPdf: string;
  /**
   * The line under a PDF's title: how it is shown [kpadopt6-media]
   */
  previewShownByBrowser: string;
  /**
   * The sentence when a picture would not load [kpadopt6-media]
   */
  previewImage: string;
  /**
   * The reason an opener is off when there is no file yet [kpadopt6-media]
   */
  previewNoFile: string;
  /**
   * The facts word of a picture file [kpadopt6-media]
   */
  docKindImage: string;
  /**
   * The facts word of a text file [kpadopt6-media]
   */
  docKindText: string;
  /**
   * The name of a document glyph whose file is missing [kpadopt6-media]
   */
  docMissing: string;
  /**
   * The name of a document glyph: "PDF document" [kpadopt6-media]
   */
  docKindName: (kind: string) => string;
  /**
   * The button that opens the picture last looked at (js/gallery.js) [kpadopt6-media]
   */
  galleryContinue: string;
  /**
   * The failed gallery's heading [kpadopt6-media]
   */
  galleryFailed: string;
  /**
   * Its sentence [kpadopt6-media]
   */
  galleryFailedText: string;
  /**
   * The empty gallery's heading [kpadopt6-media]
   */
  galleryEmpty: string;
  /**
   * Its sentence [kpadopt6-media]
   */
  galleryEmptyText: string;
  /**
   * Its sentence when pictures can be added [kpadopt6-media]
   */
  galleryEmptyAdd: string;
  /**
   * The Retry of a failed gallery or picture [kpadopt6-media]
   */
  galleryRetry: string;
  /**
   * The add tile's words [kpadopt6-media]
   */
  galleryAdd: string;
  /**
   * The busy card's words while the gallery loads [kpadopt6-media]
   */
  galleryLoading: string;
  /**
   * What a picture without a name is called [kpadopt6-media]
   */
  galleryPicture: string;
  /**
   * The name of a tile that opens a picture [kpadopt6-media]
   */
  galleryOpenPicture: (name: string) => string;
  /**
   * The name of a tile that opens a video [kpadopt6-media]
   */
  galleryOpenVideo: (name: string) => string;
  /**
   * The heading of a picture that would not load [kpadopt6-media]
   */
  galleryPictureFailed: string;
  /**
   * Its sentence in the lightbox [kpadopt6-media]
   */
  galleryPictureFailedText: string;
  /**
   * The accessible name of a picture's Retry [kpadopt6-media]
   */
  galleryRetryPicture: (name: string) => string;
  /**
   * The lightbox's previous button [kpadopt6-media]
   */
  galleryPrevious: string;
  /**
   * The lightbox's next button [kpadopt6-media]
   */
  galleryNext: string;
  /**
   * The reason previous is off at the first picture [kpadopt6-media]
   */
  galleryFirst: string;
  /**
   * The reason next is off at the last picture [kpadopt6-media]
   */
  galleryLast: string;
  /**
   * The sheet's heading for the lightbox keys [kpadopt6-media]
   */
  galleryKeyGroup: string;
  /**
   * The sheet's words for Left [kpadopt6-media]
   */
  galleryKeyPrevious: string;
  /**
   * The sheet's words for Right [kpadopt6-media]
   */
  galleryKeyNext: string;
  /**
   * The sheet's words for Home [kpadopt6-media]
   */
  galleryKeyFirst: string;
  /**
   * The sheet's words for End [kpadopt6-media]
   */
  galleryKeyLast: string;
  /**
   * The sheet's words for z [kpadopt6-media]
   */
  galleryKeyZoom: string;
  /**
   * The sheet's words for + [kpadopt6-media]
   */
  galleryKeyZoomIn: string;
  /**
   * The sheet's words for - [kpadopt6-media]
   */
  galleryKeyZoomOut: string;
  /**
   * The sheet's words for f [kpadopt6-media]
   */
  galleryKeyFull: string;
  /**
   * A uses map's name when the page gives none (js/uses-map.js) [kpadopt6-media]
   */
  usesLabel: string;
  /**
   * The sources column's name [kpadopt6-media]
   */
  usesSources: string;
  /**
   * The users column's name [kpadopt6-media]
   */
  usesUsers: string;
  /**
   * The failed map's heading [kpadopt6-media]
   */
  usesFailed: string;
  /**
   * Its sentence [kpadopt6-media]
   */
  usesFailedText: string;
  /**
   * The empty map's heading [kpadopt6-media]
   */
  usesEmpty: string;
  /**
   * Its sentence [kpadopt6-media]
   */
  usesEmptyText: string;
  /**
   * The Retry of every failed data area drawn with the state plate (heat, matrix, timeline, week, graph, table) [state-plates-1019]
   */
  stateRetry: string;
  /**
   * The sentence under a failed data area's heading when the page gives none [state-plates-1019]
   */
  stateFailedText: string;
  /**
   * The empty network's heading when the page gives no words [state-plates-1019]
   */
  graphEmpty: string;
  /**
   * The failed network's heading when the page gives no words [state-plates-1019]
   */
  graphFailed: string;
  /**
   * The failed map's Retry [kpadopt6-media]
   */
  usesRetry: string;
  /**
   * The busy card's words while the map loads [kpadopt6-media]
   */
  usesLoading: string;
  /**
   * What a source nothing uses says [kpadopt6-media]
   */
  usesNotUsed: string;
  /**
   * The exact number of uses on a source: "Used by 2" [kpadopt6-media]
   */
  usesCount: (n: number) => string;
  /**
   * The name of a node's link [kpadopt6-media]
   */
  usesOpen: (label: string) => string;
  /**
   * A source in words: "CV Developer NL, used by 2: Deloitte, Takto" [kpadopt6-media]
   */
  usesNodeSource: (label: string, n: number, names: string) => string;
  /**
   * A source nothing uses, in words [kpadopt6-media]
   */
  usesNodeUnused: (label: string) => string;
  /**
   * A user in words: "Deloitte, uses: CV Developer NL" [kpadopt6-media]
   */
  usesNodeUser: (label: string, names: string) => string;
  /**
   * A user that uses nothing, in words [kpadopt6-media]
   */
  usesNodeNothing: (label: string) => string;
  /**
   * The unsaved-changes bar's name for a screen reader (js/unsaved.js) [kpadopt5-time]
   */
  unsavedGroup: string;
  /**
   * The line while nothing is changed [kpadopt5-time]
   */
  unsavedNone: string;
  /**
   * The chip: "2 changes" [kpadopt5-time]
   */
  unsavedCount: (n: number) => string;
  /**
   * The Save button with its count: "Save 2 changes" [kpadopt5-time]
   */
  unsavedSave: (n: number) => string;
  /**
   * The Save button while nothing is changed [kpadopt5-time]
   */
  unsavedSaveNone: string;
  /**
   * The Discard button [kpadopt5-time]
   */
  unsavedDiscard: string;
  /**
   * The menu that holds Discard on a narrow bar [kpadopt5-time]
   */
  unsavedMore: string;
  /**
   * Its Discard entry [kpadopt5-time]
   */
  unsavedDiscardChanges: string;
  /**
   * What it does [kpadopt5-time]
   */
  unsavedDiscardHint: string;
  /**
   * Its entry that opens the list [kpadopt5-time]
   */
  unsavedShowChanges: string;
  /**
   * What it does [kpadopt5-time]
   */
  unsavedShowHint: string;
  /**
   * Why Save and Discard are off while nothing is changed [kpadopt5-time]
   */
  unsavedNothingToSave: string;
  /**
   * Why they are off while saving [kpadopt5-time]
   */
  unsavedSavingNow: string;
  /**
   * The list of changes' name [kpadopt5-time]
   */
  unsavedListTitle: string;
  /**
   * The button that takes you to the first changed row [kpadopt5-time]
   */
  unsavedGoFirst: string;
  /**
   * A change's name as a link to its row [kpadopt5-time]
   */
  unsavedGoTo: (key: string) => string;
  /**
   * A change's own revert button [kpadopt5-time]
   */
  unsavedRevert: string;
  /**
   * That button's accessible name [kpadopt5-time]
   */
  unsavedRevertOne: (key: string) => string;
  /**
   * A change whose row has no name [kpadopt5-time]
   */
  unsavedUnnamed: string;
  /**
   * A secret's value in the list [kpadopt5-time]
   */
  unsavedSecret: string;
  /**
   * A switch that is on, in the list [kpadopt5-time]
   */
  unsavedOn: string;
  /**
   * A switch that is off, in the list [kpadopt5-time]
   */
  unsavedOff: string;
  /**
   * An empty value, in the list [kpadopt5-time]
   */
  unsavedEmpty: string;
  /**
   * What the toast says after Discard: "Dropped 2 changes" [kpadopt5-time]
   */
  unsavedDropped: (n: number) => string;
  /**
   * Its Undo button [kpadopt5-time]
   */
  unsavedUndo: string;
  /**
   * What is said after Undo [kpadopt5-time]
   */
  unsavedRestoredUndo: (n: number) => string;
  /**
   * The toast when kept changes come back after a reload [kpadopt5-time]
   */
  unsavedRestored: (n: number) => string;
  /**
   * What is said when every change was saved [kpadopt5-time]
   */
  unsavedSavedAll: (n: number) => string;
  /**
   * The alert's first line when some were refused: "1 saved, 1 refused" [kpadopt5-time]
   */
  unsavedPartly: (saved: number, refused: number) => string;
  /**
   * The alert's first line when none was saved [kpadopt5-time]
   */
  unsavedRefused: (n: number) => string;
  /**
   * One refused change and its reason [kpadopt5-time]
   */
  unsavedWhy: (key: string, why: string) => string;
  /**
   * The alert's close [kpadopt5-time]
   */
  unsavedDismiss: string;
  /**
   * The question on following a link with unsaved changes [kpadopt5-time]
   */
  unsavedLeaveTitle: string;
  /**
   * Its sentence [kpadopt5-time]
   */
  unsavedLeaveText: (n: number) => string;
  /**
   * Its leave button [kpadopt5-time]
   */
  unsavedLeaveAccept: string;
  /**
   * Its stay button [kpadopt5-time]
   */
  unsavedLeaveStay: string;
  /**
   * The compare slider's label on the before side (js/compare.js) [kpadopt4-ideas]
   */
  compareBefore: string;
  /**
   * Its label on the after side [kpadopt4-ideas]
   */
  compareAfter: string;
  /**
   * The divider's name for a screen reader [kpadopt4-ideas]
   */
  compareSlider: (before: string, after: string) => string;
  /**
   * Where the divider stands: "40% Before, 60% After" [kpadopt4-ideas]
   */
  compareValue: (percent: number, before: string, after: string) => string;
  /**
   * A pane whose image could not be loaded [kpadopt4-ideas]
   */
  compareFailed: string;
  /**
   * The Retry button of a pane whose image could not be loaded [kpadopt4 r2]
   */
  compareRetry: string;
  /**
   * What the busy card over a compare slider says while its images load [kpadopt4 r2]
   */
  compareLoading: string;
  /**
   * The polite announcement when calm mode is switched on (js/calm.js) [kpadopt5]
   */
  calmOn: string;
  /**
   * The same, when it is switched off [kpadopt5]
   */
  calmOff: string;
  /**
   * The reason the calm switch is disabled while the system itself asks for less motion [kpadopt5]
   */
  calmSystem: string;
  /**
   * The shortcut sheet's empty state, when no shortcut is registered (js/shortcuts.js) [kpadopt5]
   */
  shortcutsEmptyTitle: string;
  /**
   * Its second line [kpadopt5]
   */
  shortcutsEmptyText: string;
  /**
   * The sheet's fixed row for the command palette key [kpadopt5]
   */
  shortcutsPalette: string;
  /**
   * The sheet's fixed row for the key that opens the sheet [kpadopt5]
   */
  shortcutsSheet: string;
  /**
   * What a screen reader hears while a chord waits for its second key [kpadopt5]
   */
  shortcutsPending: string;
  /**
   * The short line after a chord that does nothing, "No shortcut on g x" [kpadopt5]
   */
  shortcutsUnknown: (keys: string) => string;
  /**
   * The warning for a key that needs AltGr on an AZERTY keyboard [kpadopt5]
   */
  shortcutsAltGr: (keys: string, suggestion: string) => string;
  /**
   * The sheet's note on a shortcut that is not available right now [kpadopt5]
   */
  shortcutsUnavailable: string;
  /**
   * The button that opens a collapsed lede (js/lede.js) [kpadopt5]
   */
  ledeMore: string;
  /**
   * The button that folds it again [kpadopt5]
   */
  ledeLess: string;
  /**
   * The link at the end of a lede that carries `data-kp-lede-link` [kpadopt5]
   */
  ledeLearnMore: string;
  /**
   * The label before the remedy of a problem, "What now" (js/problems.js) [kpadopt5]
   */
  problemRemedy: string;
  /**
   * The button that opens the problems folded away, "Show the other 3" [kpadopt5]
   */
  problemShowOthers: (n: number) => string;
  /**
   * The button that folds them again [kpadopt5]
   */
  problemShowFewer: string;
  /**
   * The copy button on a remedy that is a command [kpadopt5]
   */
  problemCopy: string;
  /**
   * The name of a list of problems for a screen reader, "3 problems" [kpadopt5]
   */
  problemListLabel: (n: number) => string;
  /**
   * The word on a button after its work went well and the page gave no word of its own (js/flash.js) [kpadopt5]
   */
  flashDone: string;
  /**
   * The word on a button after the work was refused [kpadopt5]
   */
  flashRefused: string;
  /**
   * The word on a button after the work failed without an answer [kpadopt5]
   */
  flashFailed: string;
  /**
   * The network graph's way back to every node and every kind of link (js/graph.js) [scope-143]
   */
  graphShowAll: string;
  /**
   * Its title [scope-143]
   */
  graphShowAllTitle: string;
  /**
   * The hint under the graph, and the second half of its picture's accessible name: what a pointer, a click and the keys do [scope-143]
   */
  graphHint: string;
  /**
   * The name of the list of kinds of link over the graph [scope-143]
   */
  graphKinds: string;
  /**
   * A loading graph's sentence when the page gives none (`setGraphState(el, 'loading')`) [scope-143]
   */
  graphLoading: string;
  /**
   * The picture's name when the graph has no `aria-label` [scope-143]
   */
  graphUnnamed: string;
  breadcrumb: string;
  pagination: string;
  themePicker: string;
  themeSaveFailed: string;
  themeSaveRefused: string;
  contractDestructive: string;
  contractSemantic: string;
  themeGroupLight: string;
  themeGroupDark: string;
  themeUnknown: (requested: string, applied: string) => string;
  /**
   * The console warning when two elements of one component ask to be remembered under the same name
   */
  rememberClash: (name: string, component: string) => string;
  /**
   * The console error when a lazily loaded register did not arrive; the theme stays what it was [scope-50]
   */
  registerLoadFailed: (theme: string, href: string) => string;
  diagnosticsHeading: string;
  diagnosticsStylesheet: string;
  diagnosticsScript: string;
  diagnosticsVersion: string;
  diagnosticsThemes: string;
  diagnosticsVerdict: string;
  diagnosticsMatch: string;
  diagnosticsStylesheetBehind: (stylesheet: string, script: string) => string;
  diagnosticsScriptBehind: (stylesheet: string, script: string) => string;
  diagnosticsThemesDiffer: (version: string) => string;
  diagnosticsNoVersion: string;
  diagnosticsOnlyInStylesheet: (names: string) => string;
  diagnosticsOnlyInScript: (names: string) => string;
  diagnosticsUnknownVersion: string;
  /**
   * A three-way filter chip that must be in, as a screen reader hears it after the chip's name (js/segmented.js, `data-kp-segmented="tri"`) [kpadopt4]
   */
  triInclude: string;
  /**
   * A chip that must not be in [kpadopt4]
   */
  triExclude: string;
  /**
   * A chip that may or may not be in [kpadopt4]
   */
  triAny: string;
  /**
   * What a three-way group says after a press, "Chicken: required" [kpadopt4]
   */
  triAnnounce: (label: string, state: string) => string;
  /**
   * The line under a three-way group while nothing is chosen [kpadopt4]
   */
  triHint: string;
  /**
   * The same line once something is chosen [kpadopt4]
   */
  triSummary: (include: string[], exclude: string[]) => string;
  /**
   * What a three-way group says after Clear all [kpadopt4]
   */
  triCleared: string;
  /**
   * The minus button's name, "Portions, less" (js/quantity.js) [kpadopt4]
   */
  quantityLess: (label: string) => string;
  /**
   * The plus button's name, "Portions, more" [kpadopt4]
   */
  quantityMore: (label: string) => string;
  /**
   * The value with its unit, as a screen reader hears it, "6 portions" [kpadopt4]
   */
  quantityValue: (value: string, unit: string) => string;
  /**
   * A checklist group's count beside its heading, "4 of 9" (js/checklist.js) [kpadopt4]
   */
  checklistCount: (done: number, total: number) => string;
  /**
   * The whole list's count in its bar, "4 of 9 done" [kpadopt4]
   */
  checklistSummary: (done: number, total: number) => string;
  /**
   * The bar's button that ticks every item [kpadopt4]
   */
  checklistCheckAll: string;
  /**
   * The bar's button that takes the tick off every item [cook-1017]
   */
  checklistUncheckAll: string;
  /**
   * The bar's button that takes the ticked items away [kpadopt4]
   */
  checklistClear: string;
  /**
   * The button that brings them back [kpadopt4]
   */
  checklistUndo: string;
  /**
   * The bar's line after a clear, "3 cleared" [kpadopt4]
   */
  checklistCleared: (n: number) => string;
  /**
   * Said after Undo [kpadopt4]
   */
  checklistRestored: (n: number) => string;
  /**
   * An empty checklist [kpadopt4]
   */
  checklistEmpty: string;
  /**
   * The words under a loading checklist's spinner [kpadopt4]
   */
  checklistLoading: string;
  /**
   * A checklist that could not be read, when the page gives no message [kpadopt4]
   */
  checklistFailed: string;
  /**
   * Said when an item is ticked [kpadopt4]
   */
  checklistDone: (label: string) => string;
  /**
   * Said when an item is ticked with the keyboard and sank [kpadopt4]
   */
  checklistDoneMoved: (label: string) => string;
  /**
   * Said when an item is unticked [kpadopt4]
   */
  checklistOpen: (label: string) => string;
  /**
   * A kitchen timer's state word while it runs (js/timer.js) [kpadopt4]
   */
  timerRunning: string;
  /**
   * Its state word once it has ended [kpadopt4]
   */
  timerDoneWord: string;
  /**
   * The large line on an ended timer, its notification and its alert, "Rice is done" [kpadopt4]
   */
  timerDone: (name: string) => string;
  /**
   * What a timer says aloud when it ends and the list speaks, "Rice is done" [cook-1017]
   */
  timerSpoken: (name: string) => string;
  /**
   * The button on an ended timer that stops it and takes it away [kpadopt4]
   */
  timerStopRing: string;
  /**
   * That button's accessible name [kpadopt4]
   */
  timerAddMinuteTo: (name: string) => string;
  /**
   * The remove button's accessible name and tooltip [kpadopt4]
   */
  timerRemove: (name: string) => string;
  /**
   * A list with no timer [kpadopt4]
   */
  timerEmpty: string;
  /**
   * The busy card over a timer list that is being read [r6r8-1019]
   */
  timersLoading: string;
  /**
   * A timer list that could not be read, when the page gives no message [r6r8-1019]
   */
  timersFailed: string;
  /**
   * Said once when a timer has a minute left [kpadopt4]
   */
  timerLastMinute: (name: string) => string;
  /**
   * Said when a kitchen timer starts, with its name and its clock [kpadopt4]
   */
  timerStartedNamed: (name: string, clock: string) => string;
  /**
   * Said when a starter is pressed while its timer runs [kpadopt4]
   */
  timerAlready: (name: string) => string;
  /**
   * The count over a step shown on its own, "Step 3 of 8" (js/focus-steps.js) [kpadopt4]
   */
  focusCount: (n: number, total: number) => string;
  /**
   * The button that goes on to the next step [kpadopt4]
   */
  focusNext: string;
  /**
   * The button that goes back a step [kpadopt4]
   */
  focusPrevious: string;
  /**
   * The next button on the last step [kpadopt4]
   */
  focusDone: string;
  /**
   * The heading over what a step needs [kpadopt4]
   */
  focusNeeds: string;
  /**
   * A step's timer button, "Start a timer · 10 min" [kpadopt4]
   */
  focusTimer: (duration: string) => string;
  /**
   * The button that reads the step aloud [kpadopt4]
   */
  focusReadAloud: string;
  /**
   * The same button while it reads [kpadopt4]
   */
  focusStopReading: string;
  /**
   * The screen line while the screen is kept on [kpadopt4]
   */
  focusWakeOn: string;
  /**
   * The screen line when it is not [kpadopt4]
   */
  focusWakeOff: string;
  /**
   * The screen line in a browser that cannot keep it on [kpadopt4]
   */
  focusWakeUnsupported: string;
  /**
   * A view with no steps [kpadopt4]
   */
  focusEmpty: string;
  /**
   * The words under its spinner while the steps load [kpadopt4]
   */
  focusLoading: string;
  /**
   * Steps that could not be read, when the page gives no message [kpadopt4]
   */
  focusFailed: string;
  /**
   * The button in the middle of a wheel that picks (js/wheel.js) [kpadopt4]
   */
  wheelSpin: string;
  /**
   * The same line on a kiosk wheel, which has no button [hover-wheel-1019]
   */
  wheelIdleKiosk: string;
  /**
   * The name of a kiosk wheel for a screen reader: it is the one control [hover-wheel-1019]
   */
  wheelSpinKiosk: string;
  /**
   * Added to a resting choice's tooltip [hover-wheel-1019]
   */
  wheelResting: string;
  /**
   * The same button once the wheel has chosen [kpadopt4]
   */
  wheelSpinAgain: string;
  /**
   * The line under a wheel that has not spun yet [kpadopt4]
   */
  wheelIdle: string;
  /**
   * The line over the outcome, "The wheel chose" [kpadopt4]
   */
  wheelChose: string;
  /**
   * The line while it turns [kpadopt4]
   */
  wheelSpinning: string;
  /**
   * The line over the outcome when there was only one choice [kpadopt4]
   */
  wheelOnlyOne: string;
  /**
   * A wheel with no choices, when the page gives no sentence [kpadopt4]
   */
  wheelEmpty: string;
  /**
   * The words under its spinner while the choices load [kpadopt4]
   */
  wheelLoading: string;
  /**
   * Choices that could not be read, when the page gives no message [kpadopt4]
   */
  wheelFailed: string;
  /**
   * The word before the last name in a list of people who bring an item (js/claim.js) [kpadopt6]
   */
  claimAnd: string;
  /**
   * Why every claim button is off when the page gives no reason (js/claim.js) [kpadopt6]
   */
  claimDisabled: string;
  /**
   * Why the buttons are off while nobody is signed in (js/claim.js) [kpadopt6]
   */
  claimNoViewer: string;
  /**
   * Why the button is off on an item that has its people (js/claim.js) [kpadopt6]
   */
  claimFullReason: (max: number) => string;
  /**
   * The button's face on an item you do not bring (js/claim.js) [kpadopt6]
   */
  claimTake: string;
  /**
   * The button's face on an item you bring (a press lets go) (js/claim.js) [kpadopt6]
   */
  claimHeld: string;
  /**
   * The words of the gap, an item nobody brings (js/claim.js) [kpadopt6]
   */
  claimNobody: string;
  /**
   * The group of people on a row, as a screen reader hears it (js/claim.js) [kpadopt6]
   */
  claimBy: (names: string) => string;
  /**
   * The same for the gap (js/claim.js) [kpadopt6]
   */
  claimNobodyLabel: string;
  /**
   * The button's name on an item you bring (js/claim.js) [kpadopt6]
   */
  claimHeldLabel: (label: string) => string;
  /**
   * The button's name on an item you do not bring (js/claim.js) [kpadopt6]
   */
  claimTakeLabel: (label: string) => string;
  /**
   * The assign mode's menu button (js/claim.js) [kpadopt6]
   */
  claimAssign: string;
  /**
   * Its name for a screen reader (js/claim.js) [kpadopt6]
   */
  claimAssignOf: (label: string) => string;
  /**
   * The assign menu's heading (js/claim.js) [kpadopt6]
   */
  claimAssignHeading: string;
  /**
   * The hint of a person who is on the item (js/claim.js) [kpadopt6]
   */
  claimAssigned: string;
  /**
   * The hint of a person who is not (js/claim.js) [kpadopt6]
   */
  claimNotAssigned: string;
  /**
   * A group's count of items somebody brings (js/claim.js) [kpadopt6]
   */
  claimGroupCount: (done: number, total: number) => string;
  /**
   * The bar's line when every item has somebody (js/claim.js) [kpadopt6]
   */
  claimAllTaken: string;
  /**
   * The bar's line with the exact number of items nobody brings (js/claim.js) [kpadopt6]
   */
  claimGapsCount: (n: number) => string;
  /**
   * The filter chips' name (js/claim.js) [kpadopt6]
   */
  claimFilterLabel: string;
  /**
   * The chip that shows every item (js/claim.js) [kpadopt6]
   */
  claimFilterAll: string;
  /**
   * The chip that shows only the items nobody brings (js/claim.js) [kpadopt6]
   */
  claimFilterGaps: string;
  /**
   * The chip that shows only the items you bring (js/claim.js) [kpadopt6]
   */
  claimFilterMine: string;
  /**
   * The bar's button that takes all items nobody brings (js/claim.js) [kpadopt6]
   */
  claimTakeAll: string;
  /**
   * The bar's button that copies the items nobody brings (js/claim.js) [kpadopt6]
   */
  claimCopyGaps: string;
  /**
   * Why those two buttons are off (js/claim.js) [kpadopt6]
   */
  claimNothingOpen: string;
  /**
   * The filter plate when no item is left without somebody (js/claim.js) [kpadopt6]
   */
  claimNoneOpenTitle: string;
  /**
   * Its sentence (js/claim.js) [kpadopt6]
   */
  claimNoneOpenText: string;
  /**
   * The filter plate for Mine when it is empty (js/claim.js) [kpadopt6]
   */
  claimNoneMineTitle: string;
  /**
   * Its sentence (js/claim.js) [kpadopt6]
   */
  claimNoneMineText: string;
  /**
   * The plates' button (js/claim.js) [kpadopt6]
   */
  claimShowAll: string;
  /**
   * The Retry of a list that could not be read (js/claim.js) [kpadopt6]
   */
  claimRetry: string;
  /**
   * The line when the server refused a change and gave no reason (js/claim.js) [kpadopt6]
   */
  claimNotSaved: string;
  /**
   * The line when somebody was faster (js/claim.js) [kpadopt6]
   */
  claimLost: (name: string, label: string) => string;
  /**
   * What is said after a change (js/claim.js) [kpadopt6]
   */
  claimAnnounce: (label: string, names: string) => string;
  /**
   * What is said after the last person let go (js/claim.js) [kpadopt6]
   */
  claimAnnounceNobody: (label: string) => string;
  /**
   * The toast after you let go (js/claim.js) [kpadopt6]
   */
  claimReleased: (label: string) => string;
  /**
   * The toast after a person was taken off (js/claim.js) [kpadopt6]
   */
  claimUnassigned: (name: string, label: string) => string;
  /**
   * The toast and the line after Take every open item (js/claim.js) [kpadopt6]
   */
  claimTookAll: (n: number) => string;
  /**
   * The text Copy the open list writes (js/claim.js) [kpadopt6]
   */
  claimCopyLine: (names: string) => string;
  /**
   * What is said after the copy (js/claim.js) [kpadopt6]
   */
  claimCopied: (n: number) => string;
  /**
   * What is said when the clipboard refuses (js/claim.js) [kpadopt6]
   */
  claimCopyFailed: string;
  /**
   * The shortcut sheet's line for c (js/claim.js) [kpadopt6]
   */
  claimKeyTake: string;
  /**
   * The sheet's heading for the claim keys (js/claim.js) [kpadopt6]
   */
  claimKeyGroup: string;
  /**
   * Why c does nothing now (js/claim.js) [kpadopt6]
   */
  claimKeyReason: string;
  /**
   * The sheet's line for Shift C (js/claim.js) [kpadopt6]
   */
  claimKeyGaps: string;
  /**
   * A proposed period's length under its dates (js/availability.js) [kpadopt6]
   */
  availabilityDaysLong: (n: number) => string;
  /**
   * The answer yes, in a cell's name, the legend and the list (js/availability.js) [kpadopt6]
   */
  availabilityYes: string;
  /**
   * The answer maybe (js/availability.js) [kpadopt6]
   */
  availabilityMaybe: string;
  /**
   * The answer no (js/availability.js) [kpadopt6]
   */
  availabilityNo: string;
  /**
   * A cell nobody filled (js/availability.js) [kpadopt6]
   */
  availabilityNone: string;
  /**
   * The line over a grid nobody has answered (js/availability.js) [kpadopt6]
   */
  availabilityNobodyAnswered: string;
  /**
   * One column's exact count (js/availability.js) [kpadopt6]
   */
  availabilityCount: (yes: number, total: number) => string;
  /**
   * The line over the grid (js/availability.js) [kpadopt6]
   */
  availabilityAnswered: (n: number, total: number) => string;
  /**
   * The best column in words (js/availability.js) [kpadopt6]
   */
  availabilityBestLine: (labels: string, count: string) => string;
  /**
   * The button on the best column (js/availability.js) [kpadopt6]
   */
  availabilityChoose: string;
  /**
   * The organiser's switch between periods and days (js/availability.js) [kpadopt6]
   */
  availabilityUnitLabel: string;
  /**
   * Its first choice (js/availability.js) [kpadopt6]
   */
  availabilityByPeriod: string;
  /**
   * Its second choice (js/availability.js) [kpadopt6]
   */
  availabilityByDay: string;
  /**
   * The failed plate's heading (js/availability.js) [kpadopt6]
   */
  availabilityFailedTitle: string;
  /**
   * Its sentence (js/availability.js) [kpadopt6]
   */
  availabilityFailedText: string;
  /**
   * The empty plate's heading (js/availability.js) [kpadopt6]
   */
  availabilityEmptyTitle: string;
  /**
   * Its sentence (js/availability.js) [kpadopt6]
   */
  availabilityEmptyText: string;
  /**
   * The failed plate's button (js/availability.js) [kpadopt6]
   */
  availabilityRetry: string;
  /**
   * The grid's name when the page gives none (js/availability.js) [kpadopt6]
   */
  availabilityGroup: string;
  /**
   * The summary row's name (js/availability.js) [kpadopt6]
   */
  availabilitySummaryLabel: string;
  /**
   * Your own row's name (js/availability.js) [kpadopt6]
   */
  availabilityYou: (name: string) => string;
  /**
   * The legend's word for the best column (js/availability.js) [kpadopt6]
   */
  availabilityBestWord: string;
  /**
   * A cell's name for a screen reader (js/availability.js) [kpadopt6]
   */
  availabilityCell: (name: string, column: string, answer: string) => string;
  /**
   * The line when a change was refused (js/availability.js) [kpadopt6]
   */
  availabilityNotSaved: (label: string) => string;
  /**
   * Said when a column becomes complete (js/availability.js) [kpadopt6]
   */
  availabilityEveryone: (label: string) => string;
  /**
   * Said after a stroke (js/availability.js) [kpadopt6]
   */
  availabilityYourAnswers: (yes: number, maybe: number, no: number) => string;
  /**
   * The toast after a stroke (js/availability.js) [kpadopt6]
   */
  availabilityStroke: (n: number) => string;
  /**
   * Said after one cell changed (js/availability.js) [kpadopt6]
   */
  availabilityCellSaid: (name: string, label: string, word: string) => string;
  /**
   * The menu entry that empties a cell (js/availability.js) [kpadopt6]
   */
  availabilityClear: string;
  /**
   * The shortcut sheet's line for y (js/availability.js) [kpadopt6]
   */
  availabilityKeyYes: string;
  /**
   * The line for m (js/availability.js) [kpadopt6]
   */
  availabilityKeyMaybe: string;
  /**
   * The line for n (js/availability.js) [kpadopt6]
   */
  availabilityKeyNo: string;
  /**
   * The line for Backspace (js/availability.js) [kpadopt6]
   */
  availabilityKeyClear: string;
  /**
   * The sheet's heading (js/availability.js) [kpadopt6]
   */
  availabilityKeyGroup: string;
  /**
   * Why those keys do nothing now (js/availability.js) [kpadopt6]
   */
  availabilityKeyReason: string;
  /**
   * Why every cell is inert when the page gives no reason (js/availability.js) [kpadopt6]
   */
  availabilityDisabled: string;
  /**
   * The code's accessible name (js/qr.js) [kpadopt6]
   */
  qrLabel: string;
  /**
   * Said once, politely, when the code is drawn (js/qr.js) [kpadopt6]
   */
  qrReady: string;
  /**
   * The failed plate's heading (js/qr.js) [kpadopt6]
   */
  qrTooLongTitle: string;
  /**
   * Its sentence (js/qr.js) [kpadopt6]
   */
  qrTooLongText: string;
  /**
   * The empty plate's heading (js/qr.js) [kpadopt6]
   */
  qrEmptyTitle: string;
  /**
   * Its sentence (js/qr.js) [kpadopt6]
   */
  qrEmptyText: string;
  /**
   * The plate of a link whose time ran out (js/qr.js) [kpadopt6]
   */
  qrExpired: string;
  /**
   * Its sentence (js/qr.js) [kpadopt6]
   */
  qrExpiredText: string;
  /**
   * The cover of a code that waits to be shown (js/qr.js) [kpadopt6]
   */
  qrHidden: string;
  /**
   * The cover's button (js/qr.js) [kpadopt6]
   */
  qrShow: string;
  /**
   * The button that covers a shown code again (js/qr.js) [kpadopt6]
   */
  qrHide: string;
  /**
   * The words under the busy card while the link is made (js/qr.js) [kpadopt6]
   */
  qrLoading: string;
  /**
   * The Copy button (js/qr.js) [kpadopt6]
   */
  qrCopy: string;
  /**
   * Its result word (js/qr.js) [kpadopt6]
   */
  qrCopied: string;
  /**
   * Its refusal word (js/qr.js) [kpadopt6]
   */
  qrCopyFailed: string;
  /**
   * Why Copy is off without a link (js/qr.js) [kpadopt6]
   */
  qrNoLink: string;
  /**
   * Why Copy is off while the code is not ready (js/qr.js) [kpadopt6]
   */
  qrCopyOff: string;
  /**
   * The download button for a picture (js/qr.js) [kpadopt6]
   */
  qrDownloadPng: string;
  /**
   * The download button for a vector file (js/qr.js) [kpadopt6]
   */
  qrDownloadSvg: string;
  /**
   * The warning under a long link (js/qr.js) [kpadopt6]
   */
  qrDense: (bytes: number) => string;
  /**
   * The failed plate's button (js/qr.js) [kpadopt6]
   */
  qrRetry: string;
  /**
   * The shortcut sheet's line for Shift C (js/qr.js) [kpadopt6]
   */
  qrKey: string;
  /**
   * The sheet's heading (js/qr.js) [kpadopt6]
   */
  qrKeyGroup: string;
  /**
   * Why the key does nothing now (js/qr.js) [kpadopt6]
   */
  qrKeyReason: string;
  /**
   * The button that opens the list of times, and the sheet's title when the field has no name (js/timefield.js) [kpadopt6]
   */
  timeChoose: string;
  /**
   * The line under an empty required field (js/timefield.js) [kpadopt6]
   */
  timeRequired: string;
  /**
   * The line under a text that is not a time (js/timefield.js) [kpadopt6]
   */
  timeInvalid: string;
  /**
   * The line under a time outside the window (js/timefield.js) [kpadopt6]
   */
  timeOutside: (min: string, max: string) => string;
  /**
   * The polite line after a 12 hour time was pasted (js/timefield.js) [kpadopt6]
   */
  timeConverted: (value: string) => string;
  /**
   * What is said after the value changed (js/timefield.js) [kpadopt6]
   */
  timeSaid: (value: string) => string;
  /**
   * The name of the group of preset chips (js/timefield.js) [kpadopt6]
   */
  timePresets: string;
  /**
   * The compact plate when presets failed (js/timefield.js) [kpadopt6]
   */
  timePresetsFailed: string;
  /**
   * Its button (js/timefield.js) [kpadopt6]
   */
  timeRetry: string;
  /**
   * The list's name for a screen reader (js/timefield.js) [kpadopt6]
   */
  timeList: string;
  /**
   * The list's entry that clears the field (js/timefield.js) [kpadopt6]
   */
  timeNone: string;
  /**
   * The note on the list's entry for the current time (js/timefield.js) [kpadopt6]
   */
  timeNow: string;
  /**
   * The mark on a time close to a busy moment (js/timefield.js) [kpadopt6]
   */
  timeNear: (busy: string) => string;
  /**
   * The quiet line that says whose clock the time is (js/timefield.js) [kpadopt6]
   */
  timeZoneNote: (zone: string) => string;
  /**
   * Said when stepping up went past midnight (js/timefield.js) [kpadopt6]
   */
  timeWrappedNext: (value: string) => string;
  /**
   * Said when stepping down went past midnight (js/timefield.js) [kpadopt6]
   */
  timeWrappedPrevious: (value: string) => string;
  /**
   * The pill's word for a block, singular and plural (js/console.js) [kpadopt6]
   */
  consoleUnit: string;
  /**
   * The body's name for a screen reader (js/console.js) [kpadopt6]
   */
  consoleOutputLabel: string;
  /**
   * The prompt's name when the console has none (js/console.js) [kpadopt6]
   */
  consoleCommand: string;
  /**
   * The failed plate's heading and the reason the prompt is off then (js/console.js) [kpadopt6]
   */
  consoleHostFailed: string;
  /**
   * The failed plate's sentence (js/console.js) [kpadopt6]
   */
  consoleHostFailedText: string;
  /**
   * Why the prompt is off when the page gives no reason (js/console.js) [kpadopt6]
   */
  consoleOff: string;
  /**
   * Why the prompt is off while the context list loads (js/console.js) [kpadopt6]
   */
  consoleContextLoadingWhy: string;
  /**
   * The Run button (js/console.js) [kpadopt6]
   */
  consoleRun: string;
  /**
   * The Run button after a line the app wants confirmed (js/console.js) [kpadopt6]
   */
  consoleRunAnyway: string;
  /**
   * The line under the prompt when the page gives none (js/console.js) [kpadopt6]
   */
  consoleHint: string;
  /**
   * The context select's label when the page gives none (js/console.js) [kpadopt6]
   */
  consoleContext: string;
  /**
   * The select while its options load (js/console.js) [kpadopt6]
   */
  consoleContextLoading: string;
  /**
   * A block before the app answered (js/console.js) [kpadopt6]
   */
  consoleSending: string;
  /**
   * A queued block (js/console.js) [kpadopt6]
   */
  consoleQueued: string;
  /**
   * A running block with its timer off (js/console.js) [kpadopt6]
   */
  consoleRunning: string;
  /**
   * A running block's status with its elapsed time (js/console.js) [kpadopt6]
   */
  consoleRunningFor: (time: string) => string;
  /**
   * A finished block's exit code (js/console.js) [kpadopt6]
   */
  consoleExit: (code: number) => string;
  /**
   * The word beside exit 0 (js/console.js) [kpadopt6]
   */
  consoleDone: string;
  /**
   * The word beside a non-zero exit (js/console.js) [kpadopt6]
   */
  consoleFailed: string;
  /**
   * A block the app refused (js/console.js) [kpadopt6]
   */
  consoleRefused: string;
  /**
   * A block that was stopped (js/console.js) [kpadopt6]
   */
  consoleCancelled: string;
  /**
   * A block whose connection dropped (js/console.js) [kpadopt6]
   */
  consoleLost: string;
  /**
   * The block's button that copies its command (js/console.js) [kpadopt6]
   */
  consoleCopyCommand: string;
  /**
   * The block's button that copies its output (js/console.js) [kpadopt6]
   */
  consoleCopyOutput: string;
  /**
   * The block's button that runs its command again (js/console.js) [kpadopt6]
   */
  consoleAgain: string;
  /**
   * The lost block's button that runs it again to see (js/console.js) [kpadopt6]
   */
  consoleRecheck: string;
  /**
   * The running block's button that stops it (js/console.js) [kpadopt6]
   */
  consoleCancel: string;
  /**
   * The block's button that folds it (js/console.js) [kpadopt6]
   */
  consoleCollapse: string;
  /**
   * The block's button that opens it again (js/console.js) [kpadopt6]
   */
  consoleExpand: string;
  /**
   * A copy button's result word (js/console.js) [kpadopt6]
   */
  consoleCopied: string;
  /**
   * A copy button's refusal word (js/console.js) [kpadopt6]
   */
  consoleNotCopied: string;
  /**
   * A lost block's what (js/console.js) [kpadopt6]
   */
  consoleLostWhat: string;
  /**
   * A lost block's what now (js/console.js) [kpadopt6]
   */
  consoleLostNow: string;
  /**
   * A refusal's what when the app gave none (js/console.js) [kpadopt6]
   */
  consoleRefusedWhat: string;
  /**
   * A finished block with nothing written (js/console.js) [kpadopt6]
   */
  consoleNoOutput: string;
  /**
   * The output block's title (js/console.js) [kpadopt6]
   */
  consoleOutput: string;
  /**
   * A running block that has written nothing yet (js/console.js) [kpadopt6]
   */
  consoleWaitingOutput: string;
  /**
   * The failed plate's button (js/console.js) [kpadopt6]
   */
  consoleRetry: string;
  /**
   * The words under the busy card while the console loads (js/console.js) [kpadopt6]
   */
  consoleLoading: string;
  /**
   * The line that stands for blocks folded away (js/console.js) [kpadopt6]
   */
  consoleEarlier: (n: number) => string;
  /**
   * The empty plate's heading (js/console.js) [kpadopt6]
   */
  consoleEmptyTitle: string;
  /**
   * Its sentence (js/console.js) [kpadopt6]
   */
  consoleEmptyText: string;
  /**
   * Said when the last command succeeded (js/console.js) [kpadopt6]
   */
  consoleFinishedOk: (code: number) => string;
  /**
   * Said when the last command failed (js/console.js) [kpadopt6]
   */
  consoleFinishedFailed: (code: number) => string;
  /**
   * Said when the last command was refused (js/console.js) [kpadopt6]
   */
  consoleFinishedRefused: string;
  /**
   * Said when a line needs a second press (js/console.js) [kpadopt6]
   */
  consoleConfirmSaid: (command: string) => string;
  /**
   * The toast and the line after Clear (js/console.js) [kpadopt6]
   */
  consoleCleared: string;
  /**
   * The completion list's name (js/console.js) [kpadopt6]
   */
  consoleCompletions: string;
  /**
   * Said after Tab found several (js/console.js) [kpadopt6]
   */
  consoleMatches: (n: number) => string;
  /**
   * The shortcut sheet's line for Ctrl L (js/console.js) [kpadopt6]
   */
  consoleKeyClear: string;
  /**
   * The sheet's heading (js/console.js) [kpadopt6]
   */
  consoleKeyGroup: string;
  /**
   * The sheet's line for the focus key (js/console.js) [kpadopt6]
   */
  consoleKeyFocus: string;
  /**
   * Why the focus key does nothing now (js/console.js) [kpadopt6]
   */
  consoleKeyFocusReason: string;
  /**
   * The button that unfolds a long link under the code (js/qr.js) [kpadopt6]
   */
  qrShowAllChars: (n: number) => string;
  /**
   * The button that folds it again (js/qr.js) [kpadopt6]
   */
  qrShowLess: string;
  /**
   * The diff view's accessible name when it has none (js/diff.js) [gap-diff-view]
   */
  diffLabel: string;
  /**
   * The bar's count of files: "3 files changed" (js/diff.js) [gap-diff-view]
   */
  diffSummary: (files: number) => string;
  /**
   * The button that opens every file (js/diff.js) [gap-diff-view]
   */
  diffExpandAll: string;
  /**
   * The button that closes every file (js/diff.js) [gap-diff-view]
   */
  diffCollapseAll: string;
  /**
   * Why Expand all is off: every file is already open (js/diff.js) [gap-diff-view]
   */
  diffAllOpen: string;
  /**
   * Why Collapse all is off: every file is already closed (js/diff.js) [gap-diff-view]
   */
  diffAllClosed: string;
  /**
   * The badge of a new file (js/diff.js) [gap-diff-view]
   */
  diffStatusAdded: string;
  /**
   * The badge of a deleted file (js/diff.js) [gap-diff-view]
   */
  diffStatusRemoved: string;
  /**
   * The badge of a changed file (js/diff.js) [gap-diff-view]
   */
  diffStatusModified: string;
  /**
   * The badge of a moved file (js/diff.js) [gap-diff-view]
   */
  diffStatusRenamed: string;
  /**
   * The badge of a file that is not text (js/diff.js) [gap-diff-view]
   */
  diffStatusBinary: string;
  /**
   * A renamed file's path for a screen reader: "renamed from old/path" (js/diff.js) [gap-diff-view]
   */
  diffRenamedFrom: (from: string) => string;
  /**
   * The +N count for a screen reader: "3 lines added" (js/diff.js) [gap-diff-view]
   */
  diffAddedCount: (n: number) => string;
  /**
   * The -M count for a screen reader: "1 line removed" (js/diff.js) [gap-diff-view]
   */
  diffRemovedCount: (n: number) => string;
  /**
   * The folded context row: "12 unchanged lines" (js/diff.js) [gap-diff-view]
   */
  diffUnchanged: (n: number) => string;
  /**
   * That row's accessible name: "Show 12 unchanged lines" (js/diff.js) [gap-diff-view]
   */
  diffUnchangedName: (n: number) => string;
  /**
   * The same row once opened: "Hide 12 unchanged lines" (js/diff.js) [gap-diff-view]
   */
  diffUnchangedHide: (n: number) => string;
  /**
   * Said politely after a fold opened: "12 unchanged lines shown" (js/diff.js) [gap-diff-view]
   */
  diffUnchangedShown: (n: number) => string;
  /**
   * Said politely after a fold closed: "12 unchanged lines hidden" (js/diff.js) [gap-diff-view]
   */
  diffUnchangedHidden: (n: number) => string;
  /**
   * The row that shows the lines past the file's limit: "Show the other 1240 lines" (js/diff.js) [gap-diff-view]
   */
  diffShowMore: (n: number) => string;
  /**
   * The heading of a file that is not text (js/diff.js) [gap-diff-view]
   */
  diffBinaryTitle: string;
  /**
   * Its sentence (js/diff.js) [gap-diff-view]
   */
  diffBinaryText: string;
  /**
   * A file with no line changes (a rename or a mode change) (js/diff.js) [gap-diff-view]
   */
  diffNoLines: string;
  /**
   * A file's body name: "Changes in stacks/media/compose.yml" (js/diff.js) [gap-diff-view]
   */
  diffChangesIn: (path: string) => string;
  /**
   * The Copy path button (js/diff.js) [gap-diff-view]
   */
  diffCopyPath: string;
  /**
   * Its result word (js/diff.js) [gap-diff-view]
   */
  diffCopied: string;
  /**
   * Its refusal word (js/diff.js) [gap-diff-view]
   */
  diffCopyFailed: string;
  /**
   * The words under the busy card while the changes are read (js/diff.js) [gap-diff-view]
   */
  diffLoading: string;
  /**
   * Said politely when the changes are drawn (js/diff.js) [gap-diff-view]
   */
  diffReady: (files: number, added: number, removed: number) => string;
  /**
   * The empty plate's heading (js/diff.js) [gap-diff-view]
   */
  diffEmptyTitle: string;
  /**
   * Its sentence (js/diff.js) [gap-diff-view]
   */
  diffEmptyText: string;
  /**
   * Its sentence when one name was compared: "compose.yml is the same on both sides." (js/diff.js) [gap-diff-view]
   */
  diffEmptySame: (name: string) => string;
  /**
   * Its sentence when two names were compared (js/diff.js) [gap-diff-view]
   */
  diffEmptyBetween: (from: string, to: string) => string;
  /**
   * The failed plate's heading (js/diff.js) [gap-diff-view]
   */
  diffFailedTitle: string;
  /**
   * Its sentence (js/diff.js) [gap-diff-view]
   */
  diffFailedText: string;
  /**
   * The failed plate's button (js/diff.js) [gap-diff-view]
   */
  diffRetry: string;
  /**
   * The star row's name for a screen reader: "Rating for Lasagne" (js/rating.js) [kpadopt6]
   */
  ratingGroupName: (item: string) => string;
  /**
   * The same when the row names no item [kpadopt6]
   */
  ratingGroupNameBare: string;
  /**
   * One star's name: "4 of 5" [kpadopt6]
   */
  ratingStarName: (n: number, max: number) => string;
  /**
   * The same with its word: "4 of 5, Good" [kpadopt6]
   */
  ratingStarNameLabel: (n: number, max: number, label: string) => string;
  /**
   * A display row's name: "Rated 4 of 5" [kpadopt6]
   */
  ratingRated: (value: number, max: number) => string;
  /**
   * The same with how many people rated: "Rated 4.5 of 5 by 12 people" [kpadopt6]
   */
  ratingRatedCount: (value: number, max: number, count: number) => string;
  /**
   * The line, and the name, of a row nobody has rated: "Not rated yet" [kpadopt6]
   */
  ratingUnrated: string;
  /**
   * The caption of an input row with no rating: "Tap a star" [kpadopt6]
   */
  ratingTap: string;
  /**
   * The small button that removes the rating [kpadopt6]
   */
  ratingClear: string;
  /**
   * That button's accessible name: "Clear the rating for Lasagne" [kpadopt6]
   */
  ratingClearName: (item: string) => string;
  /**
   * The caption of a rated row without words per level: "4 of 5" [kpadopt6]
   */
  ratingOutOf: (value: number, max: number) => string;
  /**
   * An aggregated score: "4.5 (12)" [kpadopt6]
   */
  ratingAverage: (value: number, count: number) => string;
  /**
   * What is said when the rating changes: "Lasagne: 4 of 5" [kpadopt6]
   */
  ratingAnnounce: (item: string, value: number, max: number) => string;
  /**
   * What is said when the rating is removed: "Lasagne: rating removed" [kpadopt6]
   */
  ratingAnnounceCleared: (item: string) => string;
  /**
   * The undo toast's line: "Lasagne: 4 stars" [kpadopt6]
   */
  ratingToast: (item: string, value: number) => string;
  /**
   * The undo toast's line after Clear [kpadopt6]
   */
  ratingToastCleared: (item: string) => string;
  /**
   * The row's name while the saved value is read [kpadopt6]
   */
  ratingLoading: string;
  /**
   * The words that replace the stars when the value could not be read [kpadopt6]
   */
  ratingFailedRead: string;
  /**
   * Its button [kpadopt6]
   */
  ratingRetry: string;
  /**
   * The line under the row when a save was refused: "Could not save the rating for Lasagne" [kpadopt6]
   */
  ratingSaveFailed: (item: string) => string;
  /**
   * The row's description while a save is on its way [kpadopt6]
   */
  ratingSaving: string;
  /**
   * The button that saves a pending rating (commit confirm) [kpadopt6]
   */
  ratingConfirm: string;
  /**
   * The button that drops a pending rating [kpadopt6]
   */
  ratingCancel: string;
  /**
   * The line over those two buttons: "4 of 5, not saved yet" [kpadopt6]
   */
  ratingPending: (value: number, max: number) => string;
  /**
   * What a screen reader hears on a row that cannot be changed by this person [kpadopt6]
   */
  ratingReadonly: string;
  /**
   * Who wrote a quote when the card does not say: "You" (js/quote.js) [kpadopt6]
   */
  quoteBy: string;
  /**
   * The line under a quote: "You, Tuesday 14/10/2026" [kpadopt6]
   */
  quoteCaption: (by: string, when: string) => string;
  /**
   * A date with its weekday: "Tuesday 14/10/2026" [kpadopt6]
   */
  quoteDay: (weekday: string, date: string) => string;
  /**
   * How long ago, in words: "3 days ago", "today" [kpadopt6]
   */
  quoteRelative: (days: number) => string;
  /**
   * The date and how long ago together: "Tuesday 14/10/2026 (3 days ago)" [kpadopt6]
   */
  quoteBoth: (date: string, relative: string) => string;
  /**
   * The button that stops a quote from coming back [kpadopt6]
   */
  quoteHide: string;
  /**
   * The undo toast's line after a hide: "Hidden" [kpadopt6]
   */
  quoteHideDone: string;
  /**
   * What a hide that is being saved says on its button [kpadopt6]
   */
  quoteHiding: string;
  /**
   * The button that copies the words [kpadopt6]
   */
  quoteCopy: string;
  /**
   * The same button for a moment after the copy [kpadopt6]
   */
  quoteCopied: string;
  /**
   * What is said when the clipboard refused [kpadopt6]
   */
  quoteCopyFailed: string;
  /**
   * The button that unfolds a long quote [kpadopt6]
   */
  quoteShowAll: string;
  /**
   * The same button once it is open [kpadopt6]
   */
  quoteShowLess: string;
  /**
   * A list's button that shows every quote: "Show all 7" [kpadopt6]
   */
  quoteShowAllN: (n: number) => string;
  /**
   * The same button once the list is open [kpadopt6]
   */
  quoteShowFewer: string;
  /**
   * The link to the day the words were written [kpadopt6]
   */
  quoteOpen: string;
  /**
   * The words under the loading card [kpadopt6]
   */
  quoteLoading: string;
  /**
   * The empty card's title [kpadopt6]
   */
  quoteEmptyTitle: string;
  /**
   * The empty card's sentence [kpadopt6]
   */
  quoteEmptyText: string;
  /**
   * The failed card's title [kpadopt6]
   */
  quoteFailedTitle: string;
  /**
   * The failed card's sentence [kpadopt6]
   */
  quoteFailedText: string;
  /**
   * The failed card's button [kpadopt6]
   */
  quoteRetry: string;
  /**
   * A list of quotes' name for a screen reader [kpadopt6]
   */
  quoteListName: string;
  /**
   * What is said when a quote is hidden [kpadopt6]
   */
  quoteAnnounceHidden: string;
  /**
   * A row in the calm vocabulary that is still open (js/statusrow.js, `data-kp-status-vocab="calm"`) [kpadopt6]
   */
  statusOpen: string;
  /**
   * A calm row that is done [kpadopt6]
   */
  statusDone: string;
  /**
   * A calm row that is paused [kpadopt6]
   */
  statusPaused: string;
  /**
   * A calm row nothing is known about: never shown as done [kpadopt6]
   */
  statusUnknown: string;
  /**
   * A paused row's line: "Paused since 03/10/2026" [kpadopt6]
   */
  statusSinceDate: (date: string) => string;
  /**
   * The same as a length: "Paused for 3 weeks" [kpadopt6]
   */
  statusSinceFor: (duration: string) => string;
  /**
   * The same with both: "Paused since 03/10/2026 (2 weeks)" [kpadopt6]
   */
  statusSinceBoth: (date: string, duration: string) => string;
  /**
   * A length of days in words: "5 days", "3 weeks", "4 months" [kpadopt6]
   */
  statusDuration: (days: number) => string;
  /**
   * The button that takes a paused row up again [kpadopt6]
   */
  statusResume: string;
  /**
   * That button's accessible name: "Resume Beweging" [kpadopt6]
   */
  statusResumeLabel: (label: string) => string;
  /**
   * The optional button that pauses a row [kpadopt6]
   */
  statusPause: string;
  /**
   * That button's accessible name [kpadopt6]
   */
  statusPauseLabel: (label: string) => string;
  /**
   * What is said and toasted after a resume: "Beweging resumed" [kpadopt6]
   */
  statusResumed: (label: string) => string;
  /**
   * What is said and toasted after a pause: "Beweging paused" [kpadopt6]
   */
  statusPausedNow: (label: string) => string;
  /**
   * The line that gathers long-paused rows: "3 paused for more than 30 days" [kpadopt6]
   */
  statusFoldLine: (n: number, days: number) => string;
  /**
   * The empty today card's title [kpadopt6]
   */
  statusTodayEmptyTitle: string;
  /**
   * Its second sentence: nothing is waiting [kpadopt6]
   */
  statusTodayEmptyText: string;
  /**
   * The failed today card's title [kpadopt6]
   */
  statusTodayFailedTitle: string;
  /**
   * Its sentence: the fault is the system's [kpadopt6]
   */
  statusTodayFailedText: string;
  /**
   * The words under the loading card [kpadopt6]
   */
  statusTodayLoading: string;
  /**
   * The failed card's button [kpadopt6]
   */
  statusTodayRetry: string;
  /**
   * A day with something logged, in words: "logged" (js/calm-tones.js, `data-kp-tones="calm"`) [kpadopt6]
   */
  calmLogged: string;
  /**
   * A day with nothing logged, in words: "nothing logged", never "missed" [kpadopt6]
   */
  calmQuiet: string;
  /**
   * A day inside a pause: "paused" [kpadopt6]
   */
  calmPaused: string;
  /**
   * The legend's word for a logged day [kpadopt6]
   */
  calmLegendLogged: string;
  /**
   * The legend's word for a quiet day [kpadopt6]
   */
  calmLegendQuiet: string;
  /**
   * The legend's word for a paused day [kpadopt6]
   */
  calmLegendPaused: string;
  /**
   * The plate over a month with nothing logged: "Nothing logged this month" [kpadopt6]
   */
  calmEmptyTitle: string;
  /**
   * Its second sentence [kpadopt6]
   */
  calmEmptyText: string;
  /**
   * The plate over a calendar that could not be read [kpadopt6]
   */
  calmFailedTitle: string;
  /**
   * Its sentence: the fault is the system's [kpadopt6]
   */
  calmFailedText: string;
  /**
   * Its button [kpadopt6]
   */
  calmRetry: string;
  /**
   * The words of the optional caption slot under the grid: trends over the days that were logged [kpadopt6]
   */
  calmTrendCaption: string;
  /**
   * A calm uptime strip's summary: "21 of 30 days logged", with no incident count and no share down [kpadopt6]
   */
  calmSummary: (logged: number, total: number) => string;
  /**
   * The count over a question: "Question 3 of 7" (js/focus-steps.js, answer mode) [kpadopt6]
   */
  focusQuestionCount: (n: number, total: number) => string;
  /**
   * What a screen reader hears after an answer: "Answered Matig. Question 4 of 7" [kpadopt6]
   */
  focusAnswered: (label: string, n: number, total: number) => string;
  /**
   * The button that repeats yesterday's answer: "Same as yesterday: Matig" [kpadopt6]
   */
  focusSameYesterday: (label: string) => string;
  /**
   * The button that takes the usual answer: "Same as usual: Goed" [kpadopt6]
   */
  focusSameUsual: (label: string) => string;
  /**
   * The button that passes a question: "Not today" [kpadopt6]
   */
  focusSkip: string;
  /**
   * What a skipped question reads as in the summary [kpadopt6]
   */
  focusSkipped: string;
  /**
   * The next button on the last question when a summary follows [kpadopt6]
   */
  focusReview: string;
  /**
   * The summary screen's name [kpadopt6]
   */
  focusSummaryTitle: string;
  /**
   * The line under a question that takes several answers [kpadopt6]
   */
  focusMultiHint: string;
  /**
   * The name of the box of a written answer [kpadopt6]
   */
  focusTextLabel: string;
  /**
   * The word a chosen answer carries for a screen reader besides the mark [kpadopt6]
   */
  focusPickedMark: string;
  /**
   * The question when a check-in was left half way: "Continue where you stopped?" [kpadopt6]
   */
  focusResumeAsk: string;
  /**
   * Its yes [kpadopt6]
   */
  focusResumeContinue: string;
  /**
   * Its start over [kpadopt6]
   */
  focusResumeRestart: string;
  /**
   * What is said when answers are put back: "Back at question 4" [kpadopt6]
   */
  focusResumed: (n: number) => string;
  /**
   * The plate of a day with no questions [kpadopt6]
   */
  focusAnswerEmptyTitle: string;
  /**
   * Its sentence: nothing is waiting [kpadopt6]
   */
  focusAnswerEmptyText: string;
  /**
   * The plate of questions that could not be read [kpadopt6]
   */
  focusAnswerFailedTitle: string;
  /**
   * Its sentence [kpadopt6]
   */
  focusAnswerFailedText: string;
  /**
   * Its button [kpadopt6]
   */
  focusRetry: string;
  /**
   * The plate when the finished answers could not be saved [kpadopt6]
   */
  focusSaveFailedTitle: string;
  /**
   * Its sentence: nothing is lost [kpadopt6]
   */
  focusSaveFailedText: string;
  /**
   * Its button: "Try again" [kpadopt6]
   */
  focusSaveRetry: string;
  /**
   * The words under the loading card of the questions [kpadopt6]
   */
  focusAnswerLoading: string;
  /**
   * The plate of a recipe with no steps (js/focus-steps.js, read mode) [focus-tracks-1019]
   */
  focusReadEmptyTitle: string;
  /**
   * The plate of steps that could not be loaded [focus-tracks-1019]
   */
  focusReadFailedTitle: string;
  /**
   * Its sentence when the page gave no message [focus-tracks-1019]
   */
  focusReadFailedText: string;
  /**
   * The name of the lane of steps that belong to no track: "Together" [focus-tracks-1019]
   */
  focusTrackShared: string;
  /**
   * The heading over the lanes: "Tracks" [focus-tracks-1019]
   */
  focusTracksTitle: string;
  /**
   * The line under the lanes saying how to switch [focus-tracks-1019]
   */
  focusTracksHint: string;
  /**
   * The lanes' total: "5 of 9 steps done" [focus-tracks-1019]
   */
  focusTracksDone: (n: number, total: number) => string;
  /**
   * The count over a step on a track: "Pasta: step 2 of 4" [focus-tracks-1019]
   */
  focusTrackCount: (track: string, n: number, total: number) => string;
  /**
   * A step with no title, by place: "Pasta, step 2" [focus-tracks-1019]
   */
  focusTrackStepName: (track: string, n: number) => string;
  /**
   * A lane's place on its track: "Step 2 of 4" [focus-tracks-1019]
   */
  focusLaneStep: (n: number, total: number) => string;
  /**
   * A lane's state word for the track you are on [focus-tracks-1019]
   */
  focusLaneHere: string;
  /**
   * A lane's state word for a track that can go on [focus-tracks-1019]
   */
  focusLaneReady: string;
  /**
   * A lane's state word while its timer runs: "Water running" [focus-tracks-1019]
   */
  focusLaneRunning: (timer: string) => string;
  /**
   * A lane's state word while its timer is paused [focus-tracks-1019]
   */
  focusLanePaused: (timer: string) => string;
  /**
   * A lane's state word while it waits: "Waiting for Sauce" [focus-tracks-1019]
   */
  focusLaneWaiting: (names: string) => string;
  /**
   * A lane's state word when its timer has ended: "Time is up" [focus-tracks-1019]
   */
  focusLaneRinging: string;
  /**
   * A lane's state word when its track is done [focus-tracks-1019]
   */
  focusLaneFinished: string;
  /**
   * The name of the band over the step that lists the tracks asking for you [focus-tracks-1019]
   */
  focusTrackBand: string;
  /**
   * The band's line when a step timer ended: "Water is done (Pasta)" [focus-tracks-1019]
   */
  focusTrackTimerDone: (timer: string, track: string) => string;
  /**
   * The band's line when a waiting track can go on: "Pasta can go on" [focus-tracks-1019]
   */
  focusTrackReadyTitle: (track: string) => string;
  /**
   * Its sentence: "Next: Add the spaghetti" [focus-tracks-1019]
   */
  focusTrackNextUp: (step: string) => string;
  /**
   * Its sentence when the track has no step left [focus-tracks-1019]
   */
  focusTrackNothingLeft: (track: string) => string;
  /**
   * The button that goes to a track: "Go to Pasta" [focus-tracks-1019]
   */
  focusTrackGo: (track: string) => string;
  /**
   * The band's button for the track you are on [focus-tracks-1019]
   */
  focusTrackGotIt: string;
  /**
   * The title of a step that waits: "Waiting for Boil the water" [focus-tracks-1019]
   */
  focusTrackWaitTitle: (what: string) => string;
  /**
   * What a step waits for, with the time left: "07:12 left" [focus-tracks-1019]
   */
  focusTrackWaitLeft: (clock: string) => string;
  /**
   * What a step waits for, with no clock: "not done yet" [focus-tracks-1019]
   */
  focusTrackWaitOpen: string;
  /**
   * The plate of a track with no step left: "Pasta is done" [focus-tracks-1019]
   */
  focusTrackFinishedTitle: (track: string) => string;
  /**
   * Its sentence [focus-tracks-1019]
   */
  focusTrackFinishedText: string;
  /**
   * Its sentence when another track can go on: "Sauce is ready for you" [focus-tracks-1019]
   */
  focusTrackFinishedNext: (next: string) => string;
  /**
   * The next button on the last step of a track: "Finish Pasta" [focus-tracks-1019]
   */
  focusTrackFinish: (track: string) => string;
  /**
   * What a screen reader hears after a switch of track [focus-tracks-1019]
   */
  focusTrackSwitched: (track: string, place: string, text: string) => string;
  /**
   * What a screen reader hears when the step you are on stops waiting [focus-tracks-1019]
   */
  focusTrackUnlocked: (step: string) => string;
  /**
   * A step's timer button while its timer runs: "Timer running · 07:12" [focus-tracks-1019]
   */
  focusTimerRunning: (clock: string) => string;
  /**
   * The same while it is paused [focus-tracks-1019]
   */
  focusTimerPaused: (clock: string) => string;
  /**
   * The same when it ended [focus-tracks-1019]
   */
  focusTimerDone: string;
  /**
   * The shortcut sheet's line for T [focus-tracks-1019]
   */
  focusTrackKeyNext: string;
  /**
   * The shortcut sheet's line for Shift T [focus-tracks-1019]
   */
  focusTrackKeyPrevious: string;
  /**
   * The label over the usual band of a picker (js/usual-picker.js) [kpadopt6]
   */
  usualLabel: string;
  /**
   * The label over the full list under the band [kpadopt6]
   */
  usualAll: string;
  /**
   * The status line of a picker with a band: "5 usual, 33 more" [kpadopt6]
   */
  usualStatus: (usual: number, more: number) => string;
  /**
   * The row that unfolds the full list: "Show all 38", the exact number [kpadopt6]
   */
  usualShowAll: (n: number) => string;
  /**
   * The same row once the list is open [kpadopt6]
   */
  usualShowFewer: string;
  /**
   * The row that adds what was typed: "Add 'hoofdpjn' as new" [kpadopt6]
   */
  usualAdd: (label: string) => string;
  /**
   * A close name offered for what was typed: "Did you mean Hoofdpijn?" [kpadopt6]
   */
  usualDidYouMean: (name: string) => string;
  /**
   * The plate over a search that matches nothing [kpadopt6]
   */
  usualNoMatchTitle: string;
  /**
   * Its sentence when a name can be added [kpadopt6]
   */
  usualNoMatchText: string;
  /**
   * Its sentence when nothing can be added [kpadopt6]
   */
  usualNoMatchTextFixed: string;
  /**
   * What is said when typed text named an item already in the list: "Hoofdpijn was already in the list, selected" [kpadopt6]
   */
  usualSelectedExisting: (label: string) => string;
  /**
   * What is said after an add: "Hoofdpijn added" [kpadopt6]
   */
  usualAdded: (label: string) => string;
  /**
   * The undo toast's line after an add: "Added 'Neksteun'" [kpadopt6]
   */
  usualAddedToast: (label: string) => string;
  /**
   * The plate of a picker with no items at all [kpadopt6]
   */
  usualEmptyTitle: string;
  /**
   * Its sentence when a name can be added [kpadopt6]
   */
  usualEmptyText: string;
  /**
   * Its sentence when nobody can add here [kpadopt6]
   */
  usualEmptyTextFixed: string;
  /**
   * Its button [kpadopt6]
   */
  usualAddButton: string;
  /**
   * The plate of a list that could not be read [kpadopt6]
   */
  usualFailedTitle: string;
  /**
   * Its sentence [kpadopt6]
   */
  usualFailedText: string;
  /**
   * Its button [kpadopt6]
   */
  usualRetry: string;
  /**
   * The words under the loading card of a picker [kpadopt6]
   */
  usualLoading: string;
  /**
   * The check's button that says a usual choice did not happen today [kpadopt6]
   */
  usualNotToday: string;
  /**
   * Kept for consumers: a not-today button's accessible name, "Hoofdpijn: not today" [kpadopt6]
   */
  usualNotTodayName: (label: string) => string;
  /**
   * What is said when it is set: "Hoofdpijn: not today" [kpadopt6]
   */
  usualNotTodayAnnounce: (label: string) => string;
  /**
   * The word a screen reader hears after a chip that is not today [kpadopt6]
   */
  usualNotTodayMark: string;
  /**
   * The `?` sheet's words for Alt U: "Jump to the usual choices" [kpadopt6]
   */
  usualJump: string;
  /**
   * The list's line, and what is said, when a name typed is already picked: "Headache is already picked" [kpadopt6]
   */
  usualAlreadyPicked: (label: string) => string;
  /**
   * What is said when the check's "Had it" picks a choice: "Headache: had it" [kpadopt6]
   */
  usualPicked: (label: string) => string;
  /**
   * What is said when Enter took the closest name: "Fatigu: took Fatigue" [kpadopt6]
   */
  usualTookClosest: (typed: string, label: string) => string;
  /**
   * What is said when Enter lit the first "Did you mean" row: "Did you mean Fatigue? Enter picks it" [kpadopt6]
   */
  usualAsk: (label: string) => string;
  /**
   * The line under the field after an add: "Added 'Neksteun' as new" [kpadopt6]
   */
  usualAddedNote: (label: string) => string;
  /**
   * Its button [kpadopt6]
   */
  usualUndo: string;
  /**
   * That button's accessible name: "Undo adding Neksteun" [kpadopt6]
   */
  usualUndoName: (label: string) => string;
  /**
   * What is said after that Undo: "Neksteun taken out again" [kpadopt6]
   */
  usualAddUndone: (label: string) => string;
  /**
   * The heading over the check of the usual choices [kpadopt6]
   */
  usualCheckTitle: string;
  /**
   * The line under it in the rows look: what Not today means [kpadopt6]
   */
  usualCheckWhat: string;
  /**
   * The line under it in the chips look: what a press does and what Not today means [kpadopt6]
   */
  usualCheckWhatChips: string;
  /**
   * The check's button that says the choice happened today (it picks it) [kpadopt6]
   */
  usualHadIt: string;
  /**
   * The word a screen reader hears after a chip that is had it [kpadopt6]
   */
  usualHadItWord: string;
  /**
   * What is said when an answer is taken back: "Headache: not answered" [kpadopt6]
   */
  usualUnanswered: (label: string) => string;
  /**
   * The word a screen reader hears after a chip with no answer [kpadopt6]
   */
  usualUnansweredWord: string;
  /**
   * The pill's word for a log line, singular and plural (js/logviewer.js) [gap-log-viewer]
   */
  logviewerUnit: string;
  /**
   * The viewer's name for a screen reader when the page gives none (js/logviewer.js) [gap-log-viewer]
   */
  logviewerLabel: string;
  /**
   * The scrolling list's name for a screen reader (js/logviewer.js) [gap-log-viewer]
   */
  logviewerLogLabel: string;
  /**
   * The search box's label (js/logviewer.js) [gap-log-viewer]
   */
  logviewerSearch: string;
  /**
   * The search box's placeholder (js/logviewer.js) [gap-log-viewer]
   */
  logviewerSearchHint: string;
  /**
   * The count when nothing matches (js/logviewer.js) [gap-log-viewer]
   */
  logviewerSearchNone: string;
  /**
   * The count while the box is empty (js/logviewer.js) [gap-log-viewer]
   */
  logviewerSearchIdle: string;
  /**
   * The count when there are matches and none is current (js/logviewer.js) [gap-log-viewer]
   */
  logviewerMatches: (n: number) => string;
  /**
   * The count with the current match (js/logviewer.js) [gap-log-viewer]
   */
  logviewerMatchAt: (at: number, n: number) => string;
  /**
   * The button that goes to the match before (js/logviewer.js) [gap-log-viewer]
   */
  logviewerPrevious: string;
  /**
   * The button that goes to the next match (js/logviewer.js) [gap-log-viewer]
   */
  logviewerNext: string;
  /**
   * The toggle that hides lines without a match (js/logviewer.js) [gap-log-viewer]
   */
  logviewerOnly: string;
  /**
   * The toggle that wraps long lines (js/logviewer.js) [gap-log-viewer]
   */
  logviewerWrap: string;
  /**
   * The time column control's name (js/logviewer.js) [gap-log-viewer]
   */
  logviewerTimeLabel: string;
  /**
   * Its first choice, the time of day (js/logviewer.js) [gap-log-viewer]
   */
  logviewerTimeShort: string;
  /**
   * Its second choice, date and time (js/logviewer.js) [gap-log-viewer]
   */
  logviewerTimeFull: string;
  /**
   * Its third choice, no time column (js/logviewer.js) [gap-log-viewer]
   */
  logviewerTimeOff: string;
  /**
   * The level select's label (js/logviewer.js) [gap-log-viewer]
   */
  logviewerLevel: string;
  /**
   * Its first entry, every level (js/logviewer.js) [gap-log-viewer]
   */
  logviewerLevelAll: string;
  /**
   * Its other entries: 'info', 'warning' or 'error' and above (js/logviewer.js) [gap-log-viewer]
   */
  logviewerLevelFrom: (level: "info" | "warning" | "error") => string;
  /**
   * The copy button (js/logviewer.js) [gap-log-viewer]
   */
  logviewerCopy: string;
  /**
   * The copy button's result word (js/logviewer.js) [gap-log-viewer]
   */
  logviewerCopied: string;
  /**
   * The copy button's refusal word (js/logviewer.js) [gap-log-viewer]
   */
  logviewerNotCopied: string;
  /**
   * The download button (js/logviewer.js) [gap-log-viewer]
   */
  logviewerDownload: string;
  /**
   * Why copy and download are off when no line is shown (js/logviewer.js) [gap-log-viewer]
   */
  logviewerNothing: string;
  /**
   * The foot's count of lines shown (js/logviewer.js) [gap-log-viewer]
   */
  logviewerCount: (shown: number, total: number) => string;
  /**
   * The foot's note about lines pushed out by the cap (js/logviewer.js) [gap-log-viewer]
   */
  logviewerDropped: (n: number, max: number) => string;
  /**
   * The words under the busy card while the lines are read (js/logviewer.js) [gap-log-viewer]
   */
  logviewerLoading: string;
  /**
   * The empty plate's heading (js/logviewer.js) [gap-log-viewer]
   */
  logviewerEmptyTitle: string;
  /**
   * Its sentence (js/logviewer.js) [gap-log-viewer]
   */
  logviewerEmptyText: string;
  /**
   * The plate's heading when a search or level hides every line (js/logviewer.js) [gap-log-viewer]
   */
  logviewerNoMatchTitle: string;
  /**
   * Its sentence (js/logviewer.js) [gap-log-viewer]
   */
  logviewerNoMatchText: string;
  /**
   * Its button that takes the search and the level away (js/logviewer.js) [gap-log-viewer]
   */
  logviewerClear: string;
  /**
   * The failed plate's heading (js/logviewer.js) [gap-log-viewer]
   */
  logviewerFailed: string;
  /**
   * Its sentence (js/logviewer.js) [gap-log-viewer]
   */
  logviewerFailedText: string;
  /**
   * Its button (js/logviewer.js) [gap-log-viewer]
   */
  logviewerRetry: string;
  /**
   * The shortcut sheet's line for the search key (js/logviewer.js) [gap-log-viewer]
   */
  logviewerKeySearch: string;
  /**
   * The sheet's heading (js/logviewer.js) [gap-log-viewer]
   */
  logviewerKeyGroup: string;
  /**
   * The accessible name of the feed's list when the host has none (js/feed.js) [gap-activity-feed-1019]
   */
  feedLabel: string;
  /**
   * The word for one row when the page names none, "event" (js/feed.js) [gap-activity-feed-1019]
   */
  feedUnitOne: string;
  /**
   * The word for several rows, "events" (js/feed.js) [gap-activity-feed-1019]
   */
  feedUnitMany: string;
  /**
   * The heading of today's rows (js/feed.js) [gap-activity-feed-1019]
   */
  feedToday: string;
  /**
   * The heading of yesterday's rows (js/feed.js) [gap-activity-feed-1019]
   */
  feedYesterday: string;
  /**
   * The count above the list when only some rows are loaded, "24 of 129 operations" (exact numbers) (js/feed.js) [gap-activity-feed-1019]
   */
  feedShown: (shown: number, total: number, unit: string) => string;
  /**
   * A day heading's failed count, "2 failed" (js/feed.js) [gap-activity-feed-1019]
   */
  feedFailed: (n: number) => string;
  /**
   * A day heading when nothing failed, "all ok" (js/feed.js) [gap-activity-feed-1019]
   */
  feedAllOk: string;
  /**
   * A running row, as a screen reader hears it (js/feed.js) [gap-activity-feed-1019]
   */
  feedStateRunning: string;
  /**
   * A row that is a note, not an outcome, as a screen reader hears it (js/feed.js) [gap-activity-feed-1019]
   */
  feedStateInfo: string;
  /**
   * The word before who started a row, for a screen reader, "by Claude" (js/feed.js) [gap-activity-feed-1019]
   */
  feedBy: string;
  /**
   * The accessible name of the lines that open under a row (js/feed.js) [gap-activity-feed-1019]
   */
  feedMoreLabel: (title: string) => string;
  /**
   * The link inside an opened row that goes where its title would (js/feed.js) [gap-activity-feed-1019]
   */
  feedOpen: string;
  /**
   * The head line of a feed whose refresh failed while its rows are still good, "Not refreshed: the host did not answer. Rows from 14:02." (js/feed.js) [gap-activity-feed-1019]
   */
  feedStale: (reason: string, since: string) => string;
  /**
   * The button that reads a stale feed again (js/feed.js) [gap-activity-feed-1019]
   */
  feedStaleAction: string;
  /**
   * What the busy card over the feed says (js/feed.js) [gap-activity-feed-1019]
   */
  feedLoading: string;
  /**
   * The empty plate's title (js/feed.js) [gap-activity-feed-1019]
   */
  feedEmptyTitle: string;
  /**
   * The empty plate's sentence when the page gives no reason (js/feed.js) [gap-activity-feed-1019]
   */
  feedEmptyText: string;
  /**
   * The failed plate's title (js/feed.js) [gap-activity-feed-1019]
   */
  feedErrorTitle: string;
  /**
   * The failed plate's sentence when the page gives no reason (js/feed.js) [gap-activity-feed-1019]
   */
  feedErrorText: string;
  /**
   * The failed plate's Retry button (js/feed.js) [gap-activity-feed-1019]
   */
  feedRetry: string;
  /**
   * The button that loads earlier rows (js/feed.js) [gap-activity-feed-1019]
   */
  feedEarlier: string;
  /**
   * The same button while it works (js/feed.js) [gap-activity-feed-1019]
   */
  feedEarlierLoading: string;
  /**
   * The sentence when earlier rows could not be loaded (js/feed.js) [gap-activity-feed-1019]
   */
  feedEarlierError: string;
  /**
   * The button that tries again (js/feed.js) [gap-activity-feed-1019]
   */
  feedEarlierRetry: string;
  /**
   * The note under Load earlier, "Showing 24 of 129, back to 14/10/2026" (js/feed.js) [gap-activity-feed-1019]
   */
  feedBackTo: (shown: number, total: number, date: string) => string;
  /**
   * The foot when there is nothing earlier, "Nothing earlier: all 24 are shown, back to 14/10/2026" (js/feed.js) [gap-activity-feed-1019]
   */
  feedEnd: (shown: number, date: string) => string;
  /**
   * Announced when one new row arrived (js/feed.js) [gap-activity-feed-1019]
   */
  feedNew: (title: string) => string;
  /**
   * Announced when several arrived, naming the newest (js/feed.js) [gap-activity-feed-1019]
   */
  feedNewMany: (n: number, title: string) => string;
};
/**
 * @typedef {object} Strings
 * @property {string} toggleCounting  The count's name while it is still being counted (js/toggle.js) [kpadopt6-ops]
 * @property {string} tabCountCounting  The count on a tab, while it is still being counted: its name for a screen reader (js/tabbar.js, setTabCount) [gap-tab-count]
 * @property {string} tabCountFailed  The count on a tab that could not be counted, in words (the dash on the tab says nothing) [gap-tab-count]
 * @property {(label: string, count: number | null) => string} toggleOn  The line said once a toggle is switched on, "Only errors on, 12 shown" [kpadopt6-ops]
 * @property {(label: string, count: number | null) => string} toggleOff  The line said once it is switched off [kpadopt6-ops]
 * @property {(label: string, n: number) => string} toggleCount  The line said when the count beside a toggle changed (data-kp-announce="all") [kpadopt6-ops]
 * @property {(n: number) => string} toggleClear  The button that switches a group's toggles off, "Clear filters (2)" [kpadopt6-ops]
 * @property {string} toggleRefused  The word on a toggle whose change the app refused [kpadopt6-ops]
 * @property {(label: string) => string} toggleKeyWhat  What a toggle's key does in the shortcut sheet [kpadopt6-ops]
 * @property {string} toggleKeyGroup  The group a toggle's key is listed under in the shortcut sheet [kpadopt6-ops]
 * @property {string} intentNoMatch  What a palette says under "Nothing matches" when it knows nothing better [kpadopt6-ops]
 * @property {(name: string, verb: string, subjects: string[], noun: string) => string} intentNoSubject  A palette's line when the first word is a verb it knows but the rest names nothing it can act on [kpadopt6-ops]
 * @property {string} paletteNothing  The title of a palette's empty plate when no command answers [kpadopt6-ops]
 * @property {(shown: number, total: number) => string} paletteCapped  A palette's status line when more commands match than are shown [kpadopt6-ops]
 * @property {string} paletteLoading  The words under the spinner while a palette's commands are still arriving [kpadopt6-ops]
 * @property {string} paletteFailed  The title of a palette's plate when its commands could not be read [kpadopt6-ops]
 * @property {string} paletteFailedText  Its sentence when the page gives no reason [kpadopt6-ops]
 * @property {string} paletteRetry  Its action [kpadopt6-ops]
 * @property {string} versionMajor  The word on a chip whose first number rose, and in a count: "major" (js/version-delta.js) [kpadopt6-ops]
 * @property {string} versionMinor  The word for a second-number jump [kpadopt6-ops]
 * @property {string} versionPatch  The word for a jump in a later number [kpadopt6-ops]
 * @property {string} versionOther  The word for a change that has no numbers in it, such as a new digest [kpadopt6-ops]
 * @property {string} versionDowngrade  The word on a chip whose number fell [kpadopt6-ops]
 * @property {string} versionSame  The words when the two versions are the same [kpadopt6-ops]
 * @property {string} versionUnknown  The level of a chip whose versions cannot be compared [kpadopt6-ops]
 * @property {string} versionNoUpdates  The count's words when nothing is to be updated [kpadopt6-ops]
 * @property {string} versionLoading  A chip's name while the version is still being looked up [kpadopt6-ops]
 * @property {string} versionFailed  The words on a chip whose lookup failed [kpadopt6-ops]
 * @property {string} versionNone  The words where no version is tracked at all [kpadopt6-ops]
 * @property {string} versionNotes  A release-notes link's extra words for a screen reader [kpadopt6-ops]
 * @property {(from: string, to: string, level: string) => string} versionSpoken  What a screen reader hears for a jump, "from 10.11 to 12.1, major update" [kpadopt6-ops]
 * @property {(version: string) => string} versionSpokenSame  What it hears when nothing changes [kpadopt6-ops]
 * @property {(to: string) => string} versionSpokenTo  What it hears for a moving tag that only says where it goes [kpadopt6-ops]
 * @property {(from: string, to: string) => string} versionSpokenDown  What it hears for a version that went back [kpadopt6-ops]
 * @property {(from: string, to: string) => string} versionSpokenChanged  What a screen reader hears for a change that has no size, "changed from latest to nightly" [kpadopt6-ops]
 * @property {string} ackLink  The release-notes link's words on an acknowledge gate [kpadopt6-ops]
 * @property {string} ackLabel  The tick's words when the page names none [kpadopt6-ops]
 * @property {string} ackRetry  The Retry button of a gate whose notes could not be found [kpadopt6-ops]
 * @property {string} ackLooking  Why the tick is off while the notes are looked up [kpadopt6-ops]
 * @property {string} ackLocked  Why the tick is locked while the job runs [kpadopt6-ops]
 * @property {string} ackOpenFirst  Why the tick is off until the notes were opened [kpadopt6-ops]
 * @property {(label: string) => string} ackReason  Why the guarded button is off, with the tick's own words [kpadopt6-ops]
 * @property {(remaining: number, total: number) => string} ackReasonMany  Why it is off when several gates guard it, "2 of 3 release notes still to read" [kpadopt6-ops]
 * @property {string} ackEnabled  Said once when the last gate is ticked and the button comes on [kpadopt6-ops]
 * @property {(remaining: number) => string} ackDisabled  Said once when a tick is taken away [kpadopt6-ops]
 * @property {string} ackNoLink  A gate's line when no release notes link was found [kpadopt6-ops]
 * @property {(n: number) => string} deplistCount  The list's default heading, the exact count (js/deplist.js) [kpadopt6-ops]
 * @property {string} deplistReading  The heading while the rows are still being read [kpadopt6-ops]
 * @property {(from: string, to: string) => string} deplistNeeds  A row as a screen reader hears it, "paperless needs postgres" [kpadopt6-ops]
 * @property {(from: string, to: string) => string} deplistReaches  The same for a list that says what reaches what [kpadopt6-ops]
 * @property {string} deplistNeedsWord  The word between the names of a cycle, "paperless needs redis needs paperless" [kpadopt6-ops]
 * @property {(n: number) => string} deplistMore  The button that unfolds the rest, with the exact number of rows [kpadopt6-ops]
 * @property {string} deplistFewer  The button that folds it again [kpadopt6-ops]
 * @property {string} deplistEmpty  The empty plate's title when the page gives no sentence [kpadopt6-ops]
 * @property {string} deplistFailed  The failed plate's title [kpadopt6-ops]
 * @property {string} deplistFailedText  Its sentence when the page gives no reason [kpadopt6-ops]
 * @property {string} deplistRetry  Its action [kpadopt6-ops]
 * @property {string} deplistLoading  The words under the busy card's spinner [kpadopt6-ops]
 * @property {string} deplistRunning  The word for a needed thing that is running [kpadopt6-ops]
 * @property {string} deplistStopped  The word for a needed thing that is stopped [kpadopt6-ops]
 * @property {string} deplistUnknown  The word for a needed thing whose state is not known [kpadopt6-ops]
 * @property {string} deplistCopy  The button that copies the rows as text [kpadopt6-ops]
 * @property {string} deplistCopied  Its answer once the rows are on the clipboard [kpadopt6-ops]
 * @property {string} traceLabel  The hop list's accessible name [kpadopt6-ops]
 * @property {string} traceAllowed  The summary line when every hop lets the connection through [kpadopt6-ops]
 * @property {(hop: string, rule: string) => string} traceBlocked  The summary line when a hop refuses it, with the deciding rule when there is one [kpadopt6-ops]
 * @property {string} traceNotAll  The summary line when a hop could not be measured [kpadopt6-ops]
 * @property {string} traceAllowedWord  A hop's verdict word, allowed [kpadopt6-ops]
 * @property {string} traceRefusedWord  A hop's verdict word, refused [kpadopt6-ops]
 * @property {string} traceNotReached  The word for a hop after a refusal, which the connection never reaches [kpadopt6-ops]
 * @property {string} traceNotMeasured  The word for a hop whose verdict is not known [kpadopt6-ops]
 * @property {(rule: string) => string} traceRule  The deciding rule's chip, "Rule IoT to media" [kpadopt6-ops]
 * @property {string} traceRunning  The words under the busy card's spinner while the path is checked [kpadopt6-ops]
 * @property {string} traceEmpty  The empty plate's title before a question was asked [kpadopt6-ops]
 * @property {string} traceEmptyText  Its sentence [kpadopt6-ops]
 * @property {string} traceFailed  The failed plate's title [kpadopt6-ops]
 * @property {string} traceFailedText  Its sentence when the page gives no reason [kpadopt6-ops]
 * @property {string} traceRetry  Its action [kpadopt6-ops]
 * @property {string} traceCopy  The button that copies the trace as text [kpadopt6-ops]
 * @property {string} traceCopied  Its answer once the text is on the clipboard [kpadopt6-ops]
 * @property {string} traceSwap  The button that swaps source and target [kpadopt6-ops]
 * @property {string} traceKeySwap  What the swap key does in the shortcut sheet [kpadopt6-ops]
 * @property {string} traceKeyGroup  The group the swap key is listed under [kpadopt6-ops]
 * @property {(name: string) => string} entitySelect  A card's tick, named for a screen reader, "Select media" [kpadopt6-ops]
 * @property {string} entityFailed  The state word of a card that could not be read [kpadopt6-ops]
 * @property {string} entityNotMeasured  What a card says where it has nothing measured [kpadopt6-ops]
 * @property {(n: number) => string} entityMoreFlags  The "+2" button's accessible name [kpadopt6-ops]
 * @property {string} entityFlagsLabel  The name of the menu the "+2" opens [kpadopt6-ops]
 * @property {string} entityRetry  A failed card's or grid's Retry [kpadopt6-ops]
 * @property {string} entityMore  The button that opens the rest of a card's actions [kpadopt6-ops]
 * @property {string} entityMoreLabel  The name of that menu [kpadopt6-ops]
 * @property {string} entityLoading  The words under the busy card's spinner while the cards are read [kpadopt6-ops]
 * @property {string} entityGridFailed  The failed plate's title for a grid of cards [kpadopt6-ops]
 * @property {string} entityGridFailedText  Its sentence when the page gives no reason [kpadopt6-ops]
 * @property {string} entityGridEmpty  The empty plate's title for a grid with no cards [kpadopt6-ops]
 * @property {string} entityNew  The empty plate's action [kpadopt6-ops]
 * @property {(name: string) => string} entityStar  The favourite button's name, "Favourite jellyfin" [gap-entity-tiles-1019]
 * @property {string} entityOpensNew  What a screen reader adds to a link that opens a new tab [gap-entity-tiles-1019]
 * @property {string} entityTrend  The trend row's label when the data gives none [gap-entity-tiles-1019]
 * @property {string} entityTrendNone  What the trend row says where there are fewer than two readings [gap-entity-tiles-1019]
 * @property {(label: string, text: string) => string} entityTrendName  The trend line's text alternative, "CPU, last 24 h, now 34%" [gap-entity-tiles-1019]
 * @property {string} entityTileFailed  What a tile says where its app could not be read [gap-entity-tiles-1019]
 * @property {string} entityTileLoading  The busy card's words while a board of tiles is read [gap-entity-tiles-1019]
 * @property {string} entityTileGridFailed  The failed plate's title for a board of tiles [gap-entity-tiles-1019]
 * @property {string} entityTileGridEmpty  The empty plate's title for a board with no tile [gap-entity-tiles-1019]
 * @property {string} entityTileNew  The empty board's action [gap-entity-tiles-1019]
 * @property {string} alertSuccess
 * @property {string} alertWarning
 * @property {string} alertInfo
 * @property {string} alertError
 * @property {string} attentionCritical  The word a screen reader hears before a critical problem on the attention band (setAttention) [scope-143]
 * @property {string} attentionWarning  The same, before a warning [scope-143]
 * @property {string} attentionInfo  The same, before an information [scope-143]
 * @property {(verb: string, duration: string) => string} agoText  A ticking freshness line, "updated 12 s ago" (js/freshness.js) [scope-143]
 * @property {(verb: string) => string} agoNever  The same line with no moment yet, "not updated yet" [scope-143]
 * @property {(n: number) => string} agoSeconds  A duration's seconds part, "12 s" [scope-143]
 * @property {(n: number) => string} agoMinutes  Its minutes part, "2 min" [scope-143]
 * @property {(n: number) => string} agoHours  Its hours part, "3 h" [scope-143]
 * @property {(n: number) => string} agoDays  Its days part, "1 day", "2 days" [scope-143]
 * @property {string} agoVerb  The verb a freshness line uses when its element names none [scope-143]
 * @property {(text: string) => string} agoStale  What a freshness line with `data-kp-ago-announce="state"` announces once it turns stale [scope-143]
 * @property {(text: string) => string} agoFresh  What it announces once it is fresh again [scope-143]
 * @property {string} agoInLiveRegion  The console warning for a ticking line inside a live region [scope-143]
 * @property {string} stepPending  A step list's waiting step, as a screen reader hears it (js/steplist.js)
 * @property {string} stepRunning  The step being run
 * @property {string} stepDone  A finished step
 * @property {string} stepFailed  A step that failed
 * @property {string} stepSkipped  A step that was left out
 * @property {string} stepCurrent  The step a journey stands at now, neither running nor done (`data-state="current"`) [kpadopt4]
 * @property {(title: string, state: string) => string} stepAnnounce  What the list's status region says when a step changes, "Pull the images: running"
 * @property {string} statusOk  A status row that is fine, as a screen reader hears it (js/statusrow.js)
 * @property {string} statusLoading  A status row still being read
 * @property {string} statusFailed  A status row that failed
 * @property {string} statusEmpty  A status row with nothing to show
 * @property {string} statusWarn  A status row that passes with a reservation, `data-state="warn"` [kpadopt4-rows]
 * @property {string} cardLoading  What a loading card's busy card says under its spinner (js/card-state.js) [kpadopt4-rows]
 * @property {string} cardEmpty  An empty card's note when the page gives no title [kpadopt4-rows]
 * @property {string} cardFailed  A failed card's note when the page gives no title [kpadopt4-rows]
 * @property {string} cardRetry  The Try again button of a failed card [kpadopt4-rows]
 * @property {string} statusRetry  The Retry button of a failed status row
 * @property {(label: string) => string} statusRetryLabel  That button's accessible name, "Retry vault"
 * @property {(label: string, state: string, detail: string) => string} statusAnnounce  What the list's status region says when a row changes, "vault: failed, The host did not answer"
 * @property {(count: number, failed: number) => string} statusAnnounceMany  What it says when more than three rows change in one tick, "12 updated, 2 failed"
 * @property {string} facetOnly  The "only" button's text on a facet row (js/facets.js)
 * @property {(label: string) => string} facetOnlyLabel  That button's accessible name, "Show only Host"
 * @property {string} facetAll  The "Everything" row's text when the group gives none
 * @property {(label: string) => string} facetShowingOnly  What a facet group's status region says after "only", "Showing only Host"
 * @property {string} facetShowingAll  What it says after "Everything"
 * @property {string} idmarkUnknown  The glyph of an identity mark with no name (js/idmark.js) [kpadopt4-data]
 * @property {string} idmarkOnline  A mark's status word, online (js/idmark.js) [kpadopt5-people]
 * @property {string} idmarkAway  A mark's status word, away
 * @property {string} idmarkBusy  A mark's status word, busy
 * @property {(name: string, status: string) => string} idmarkLabel  A mark with a status as a screen reader hears it, "Ada, online"
 * @property {(count: number) => string} idmarkGroup  A group of marks, named by its size, "5 people"
 * @property {(count: number) => string} idmarkMore  The "+N" button of a group, "Show the 3 more people"
 * @property {(count: number) => string} idmarkMoreTitle  The heading of the list the "+N" opens, "3 more people"
 * @property {string} passkeysLoading  What the busy card over the sign-in methods says (js/passkeys.js) [kpadopt5-people]
 * @property {string} passkeysListLabel  The sign-in methods list's name for a screen reader
 * @property {string} passkeysEmptyTitle  The empty sign-in methods list
 * @property {string} passkeysEmptyText  Its sentence, what a passkey is for
 * @property {string} passkeysRegister  The Register button of the empty list
 * @property {string} passkeysError  A sign-in methods list that could not be read, when the page gives no sentence
 * @property {string} passkeysRetry  The Retry button of a failed list
 * @property {(kind: string) => string} passkeysKind  The word for a kind of way in: passkey, magic link, password, token
 * @property {(date: string) => string} passkeysAdded  When a way was added, "Added 12/10/2026"
 * @property {string} passkeysLastUsedVerb  The verb of the last-used line, "Last used" (then "3 days ago")
 * @property {string} passkeysNeverUsed  A way that was never used
 * @property {string} passkeysCurrent  The badge on the way this session signed in with
 * @property {string} passkeysCurrentTitle  Its tooltip
 * @property {(days: number) => string} passkeysUnused  The badge on a way unused for long, "Unused for 120 days"
 * @property {string} passkeysUnusedTitle  Its tooltip: a candidate to remove
 * @property {string} passkeysRename  The Rename button
 * @property {(label: string) => string} passkeysRenameOf  Rename, as a screen reader hears it with the way's name
 * @property {string} passkeysRenameLabel  The name field's label while renaming
 * @property {string} passkeysSave  Save a new name
 * @property {string} passkeysCancel  Leave renaming
 * @property {string} passkeysNameRequired  The line under a name left empty
 * @property {(label: string) => string} passkeysRenamed  Announced after a rename
 * @property {(why: string) => string} passkeysRenameRefused  A rename the server refused, with its reason
 * @property {string} passkeysRemove  The Remove button
 * @property {string} passkeysRemoving  The Remove button while it works
 * @property {(label: string) => string} passkeysRemoveOf  Remove, as a screen reader hears it with the way's name
 * @property {(label: string) => string} passkeysRemoveTitle  The confirmation's question, "Remove Work laptop?"
 * @property {(label: string) => string} passkeysLosesWay  The first line of what a removal loses
 * @property {string} passkeysLosesLast  What removing the only way in loses
 * @property {string} passkeysLosesCurrent  What removing the way in use loses
 * @property {(label: string) => string} passkeysRemoved  Announced after a removal
 * @property {(why: string) => string} passkeysRefused  A removal the server refused, "Not removed: why"
 * @property {string} passkeysRefusedFallback  The reason when a refusal gave none
 * @property {string} passkeysLastWay  The reason Remove is off on the only way in, and what to do first
 * @property {string} passkeysOnlyWay  The short note in the place of Remove on the only way in (the `note` look)
 * @property {string} passkeysLastWayWarn  The warning on the only way in when removing it is allowed
 * @property {string} passkeysInsecureTitle  The warning heading when the page is not secure
 * @property {string} passkeysInsecureText  What to do about it
 * @property {string} threadLoading  What the busy card over a conversation says (js/thread.js) [kpadopt5-people]
 * @property {string} threadLabel  The conversation's name for a screen reader
 * @property {string} threadEmptyTitle  The empty conversation
 * @property {string} threadEmptyText  Its sentence
 * @property {string} threadAdd  The Add button of the empty conversation
 * @property {string} threadError  A conversation that could not be read, when the page gives no sentence
 * @property {string} threadRetry  The Retry button of a failed conversation, and of a message not sent
 * @property {string} threadYou  The sender of an outgoing message with no name
 * @property {string} threadUnknownSender  The sender of an incoming message with no name
 * @property {(sender: string, moment: string) => string} threadBubbleLabel  A message's label, "From Acme HR, 13/10/2026 14:02"
 * @property {(weekday: string, date: string) => string} threadDay  A day divider, "Monday 13/10/2026"
 * @property {(count: number) => string} threadAttachments  The attachments list's name, "2 attachments"
 * @property {(lines: number) => string} threadShowAll  The button under a folded message, "Show all 48 lines"
 * @property {string} threadShowLess  The same button when the message is whole
 * @property {string} threadCopy  The Copy text button of a message
 * @property {(sender: string) => string} threadCopyOf  Copy, as a screen reader hears it with the sender
 * @property {string} threadCopyFailed  The Copy button when the clipboard refused
 * @property {string} threadSending  The line on a message being sent
 * @property {(why: string) => string} threadNotSent  The line on a message that was not sent, "Not sent: why"
 * @property {string} threadNotSentFallback  The reason when none was given
 * @property {string} threadJump  The Jump to newest button
 * @property {string} chartMedian  The words in front of a median's value, "median" (js/chart.js) [kpadopt5-people]
 * @property {(have: number, need: number, unit: string) => string} chartFew  The sentence over a chart with too few readings, "3 of 7 days needed before a trend means something"
 * @property {string} chartFewUnit  What the readings are called in it when the page names none
 * @property {string} chartAnnotationKey  The key above a chart's plot, for a screen reader
 * @property {(tone: string) => string} chartKindTone  The key's word for an event that has a tone and no kind
 * @property {string} chartRunsKey  The key's note on a ringed run
 * @property {(label: string, from: string, until: string) => string} chartMarkBand  The tooltip of a band, "Maintenance, from … to …"
 * @property {(from: string, until: string) => string} chartBandWhen  The time of a band in the tooltip, "14:00 to 15:30"
 * @property {(count: number) => string} chartEdgeBefore  The arrow for events before the window, "2 events before this window"
 * @property {(count: number) => string} chartEdgeAfter  The arrow for events after the window
 * @property {(kind: string) => string} chartKindOn  A key item's tooltip, "Show or hide update events"
 * @property {string} stackbarRest  The legend row of a share bar's free rest (js/stackbar.js)
 * @property {string} stackbarOther  The legend row the parts too small to draw are merged into
 * @property {(label: string, value: string, percent: string) => string} stackbarPart  One part, as a screen reader and the hover card read it
 * @property {(value: string) => string} stackbarOver  The free row when the parts add up to more than the whole
 * @property {(parts: string[]) => string} stackbarLabel  The bar's accessible name: every part in order
 * @property {string} stackbarLoading  A share bar being read
 * @property {string} stackbarEmpty  A share bar with nothing read yet
 * @property {string} stackbarError  A share bar that could not be read
 * @property {string} stackbarRetry  The Retry button of a failed share bar
 * @property {string} bulletsLoading  What the busy card over a bullet list says (js/bullets.js)
 * @property {string} bulletsEmpty  A bullet list with no rows, when the page gives no reason
 * @property {string} bulletsError  A bullet list that could not be read, when the page gives no sentence
 * @property {string} bulletsRetry  The Retry button of a failed bullet list
 * @property {string} bulletsUnknown  A row with nothing to measure, for a screen reader
 * @property {(figure: string, threshold: string) => string} bulletsValueText  A row's meter as a screen reader reads it
 * @property {string} codeLabel  A code block's accessible name when it has no title (js/code.js)
 * @property {string} codeWrap  The code block's Wrap toggle
 * @property {string} codePretty  The JSON view laid out with indents
 * @property {string} codeRaw  The JSON view as it came
 * @property {string} codeFormat  The accessible name of the Pretty / Raw choice
 * @property {(lines: number) => string} codeShowAll  The button under a folded block
 * @property {string} codeShowLess  The same button once the block is open
 * @property {(lines: number) => string} codeLines  The block's line count beside its title
 * @property {string} codeEmpty  A block with nothing in it
 * @property {(size: string) => string} codeBinary  A block whose content is not text
 * @property {(shown: string, total: string) => string} codeTruncated  The foot of a block that holds only the start
 * @property {string} codeLoading  What the busy card over a block says
 * @property {string} codeError  A block that could not be read, when the page gives no sentence
 * @property {(title: string) => string} codeCopiedAnnouncement  Announced after a copy, naming the block
 * @property {(n: number) => string} rowSelectCount  Said after a tick, a range or a clear: how many rows are selected (js/row-select.js) [kpadopt5]
 * @property {(n: number) => string} rowSelectSkipped  Said after a range that passed rows that cannot be selected [kpadopt5]
 * @property {(n: number) => string} rowSelectAllShown  Said after Ctrl+A: every row the table shows is ticked [kpadopt5]
 * @property {(label: string, at: number, of: number) => string} rowSelectCursor  Said when the cursor moves to a row, "INC-4471, row 3 of 12" [kpadopt5]
 * @property {string} rowSelectMode  Said when a long press starts selection mode on a phone [kpadopt5]
 * @property {string} rowSelectModeEnd  Said when selection mode ends [kpadopt5]
 * @property {string} rowKeysMove  The hint line under a selectable table: j and k [kpadopt5]
 * @property {string} rowKeysTick  The hint line: x [kpadopt5]
 * @property {string} rowKeysRange  The hint line: Shift and x [kpadopt5]
 * @property {string} rowKeysClear  The hint line: Esc [kpadopt5]
 * @property {string} rowKeysOpen  The hint line: Enter [kpadopt5]
 * @property {string} rowKeysAll  The hint line: Ctrl and A, when the table offers it [kpadopt5]
 * @property {string} selectionLabel  The accessible name of a selection bar (js/selection-bar.js) [kpadopt5]
 * @property {(n: number) => string} selectionCount  The bar's count, "3 selected" [kpadopt5]
 * @property {string} selectionClear  The bar's button that ticks nothing: it says what it does [kpadopt5]
 * @property {string} selectionClearTitle  Its tooltip, naming the key [kpadopt5]
 * @property {(n: number) => string} selectionMore  The names line when not every name fits, "+4 more" with the exact number [kpadopt5]
 * @property {string} selectionFewer  The same button once the whole list is open [kpadopt5]
 * @property {string} selectionActions  The phone's button that opens the bar's actions [kpadopt5]
 * @property {(total: number) => string} selectionMatching  The offer beyond what is shown, "Select all 312 that match" [kpadopt5]
 * @property {(total: number) => string} selectionMatchingOn  The bar's line once every match is selected [kpadopt5]
 * @property {(n: number) => string} selectionOnlyShown  The button that takes the offer back, "Select only the 40 shown" [kpadopt5]
 * @property {(words: string) => string} selectionBusy  Said when a bar action starts, "Backing up 3…" is the page's own words; this is the fallback [kpadopt5]
 * @property {(done: number, refused: number) => string} selectionResult  The bar's line after an action some rows refused, "2 done, 1 refused" [kpadopt5]
 * @property {(n: number) => string} selectionDropped  Said when selected rows are gone after a refresh [kpadopt5]
 * @property {(can: number, total: number) => string} selectionCan  The reason an action is off for some rows, "2 of 3 can be updated" with the page's verb after it [kpadopt5]
 * @property {string} selectionCleared  Said when the selection is cleared [kpadopt5]
 * @property {string} undoToastUndo  The Undo button of an undo toast (js/undo-toast.js) [kpadopt5]
 * @property {(key: string) => string} undoToastKeyTitle  Its tooltip, naming the key [kpadopt5]
 * @property {(text: string, seconds: number) => string} undoToastAnnounce  Said once when the toast opens, "Deleted. Undo available for 6 seconds" [kpadopt5]
 * @property {(seconds: number) => string} undoToastClock  The text clock, "5 s" [kpadopt5]
 * @property {string} undoToastRestored  The toast's line after an undo [kpadopt5]
 * @property {string} undoToastSaving  The toast's line while the commit is on its way [kpadopt5]
 * @property {(why: string) => string} undoToastFailed  The toast's line when the commit failed, "Not done: why" [kpadopt5]
 * @property {string} undoToastDismiss  The accessible name of the toast's close button [kpadopt5]
 * @property {string} newpillUnitOne  The word for one new row on a live list (js/newpill.js) [kpadopt5]
 * @property {string} newpillUnitMany  The word for several [kpadopt5]
 * @property {(n: number, unit: string) => string} newpillNew  The pill, "12 new lines" [kpadopt5]
 * @property {string} newpillBackTail  The pill's second half on a list that grows at the end [kpadopt5]
 * @property {string} newpillBackHead  The pill's second half on a list that grows at the top [kpadopt5]
 * @property {(n: number, unit: string) => string} newpillAnnounce  Said at most every ten seconds while the list is read elsewhere [kpadopt5]
 * @property {string} newpillPause  The toolbar button that stops following [kpadopt5]
 * @property {string} newpillResume  The toolbar button that follows again, shown while paused [kpadopt5]
 * @property {(time: string) => string} newpillSince  The divider at the first unseen row, "New since 14:32" [kpadopt5]
 * @property {string} newpillPausedNote  Said when following is paused [kpadopt5]
 * @property {string} newpillFollowing  Said when following resumes [kpadopt5]
 * @property {string} payloadEmpty  A payload cell with nothing in it (js/code.js, cell) [kpadopt5]
 * @property {(size: string) => string} payloadBinary  A payload cell that holds binary data, "binary, 2.1 kB" [kpadopt5]
 * @property {string} payloadOpen  The button that opens a long payload in a dialog [kpadopt5]
 * @property {string} payloadDownload  The link that downloads a binary payload when the page gives a URL [kpadopt5]
 * @property {(lines: number) => string} payloadShowAll  The cell's fold button, "Show all 40 lines" [kpadopt5]
 * @property {string} payloadCopyFull  The cell's copy button: it copies the whole value, folded or not [kpadopt5]
 * @property {(n: number) => string} jsonValidKeys  The quiet line under a valid object, "Valid JSON · 3 keys" [kpadopt5]
 * @property {(n: number) => string} jsonValidItems  The same for a list [kpadopt5]
 * @property {string} jsonValidValue  The same for a bare value [kpadopt5]
 * @property {(line: number, column: number, why: string) => string} jsonInvalid  The error line, "Line 3, column 12: expected ','" [kpadopt5]
 * @property {(why: string) => string} jsonInvalidPlain  The error line when the place is not known [kpadopt5]
 * @property {string} jsonPlainText  The line under text that is not JSON where plain text is allowed [kpadopt5]
 * @property {string} jsonEmpty  The line under an empty field [kpadopt5]
 * @property {string} jsonFormat  The Format button [kpadopt5]
 * @property {string} jsonMinify  The Minify button [kpadopt5]
 * @property {string} jsonWrap  The Wrap toggle [kpadopt5]
 * @property {string} jsonCopy  The Copy button [kpadopt5]
 * @property {string} jsonTabHint  The help line under the field: how to leave it with the keyboard [kpadopt5]
 * @property {string} jsonGutter  The accessible name of the line numbers [kpadopt5]
 * @property {string} jsonBlocked  The validity message that stops a form with broken JSON [kpadopt5]
 * @property {(why: string) => string} jsonSchemaFailed  The error line when the page's shape check refuses valid JSON [kpadopt5]
 * @property {string} jsonFormatted  Said after Format [kpadopt5]
 * @property {string} reqLabel  The accessible name of the request list (js/request-inspector.js) [kpadopt5]
 * @property {string} reqEmptyTitle  The empty list's heading [kpadopt5]
 * @property {string} reqEmptyText  Its sentence [kpadopt5]
 * @property {string} reqSendTest  Its button [kpadopt5]
 * @property {string} reqFailedTitle  The failed list's heading [kpadopt5]
 * @property {string} reqLoading  The busy card's words [kpadopt5]
 * @property {string} reqHeaders  The headers block's heading [kpadopt5]
 * @property {string} reqBody  The body block's title [kpadopt5]
 * @property {string} reqNoBody  A request without a body [kpadopt5]
 * @property {string} reqNoHeaders  A request without headers [kpadopt5]
 * @property {string} reqCopyCurl  Copy as curl [kpadopt5]
 * @property {string} reqCopyJson  Copy as JSON [kpadopt5]
 * @property {string} reqCopyBody  Copy the body [kpadopt5]
 * @property {string} reqCopyCurlSecrets  The separate button that copies the curl with the real secrets [kpadopt5]
 * @property {(what: string) => string} reqCopyDone  Said after a copy, "curl copied, secrets masked" is built from this and the next [kpadopt5]
 * @property {string} reqMaskedNote  Added to a copy that masked the secrets [kpadopt5]
 * @property {string} reqSecretShow  The button that shows one masked header on screen [kpadopt5]
 * @property {string} reqSecretHide  The same, once shown [kpadopt5]
 * @property {string} reqSecretMasked  What a screen reader hears for a masked value [kpadopt5]
 * @property {(n: number) => string} reqShowOlder  The button under the list, "Show 12 older" [kpadopt5]
 * @property {string} reqFilterLabel  The accessible name of the status filter [kpadopt5]
 * @property {string} reqFilterAll  The filter's first choice [kpadopt5]
 * @property {string} reqToneSuccess  The word beside a 2xx status [kpadopt5]
 * @property {string} reqToneInfo  The word beside a 3xx status [kpadopt5]
 * @property {string} reqToneWarning  The word beside a 4xx status [kpadopt5]
 * @property {string} reqToneError  The word beside a 5xx status [kpadopt5]
 * @property {string} reqToneNone  The word beside a request without a status [kpadopt5]
 * @property {string} reqSourceTest  The badge of a request the page sent itself [kpadopt5]
 * @property {(shown: number, total: number) => string} reqCount  The line above the list, "20 of 31 requests" [kpadopt5]
 * @property {string} reqNoMatch  The list when the filter shows nothing [kpadopt5]
 * @property {(method: string, path: string, status: string) => string} reqSummary  A request's accessible name [kpadopt5]
 * @property {string} noticesInbox  The counter button's word (js/notices.js) [kpadopt5-notices]
 * @property {string} noticesTitle  The panel's heading [kpadopt5-notices]
 * @property {string} noticesPageTitle  The Inbox page's heading [kpadopt5-notices]
 * @property {string} noticesList  The accessible name of the list of notices [kpadopt5-notices]
 * @property {(n: number) => string} noticesUnread  The counter's label, "5 unread" [kpadopt5-notices]
 * @property {string} noticesNoneUnread  The counter's label with nothing unread [kpadopt5-notices]
 * @property {(why: string) => string} noticesCounterError  The counter's label when the notices could not be read [kpadopt5-notices]
 * @property {(inbox: string, label: string, quiet: string) => string} noticesCounterName  The counter button's accessible name, "Inbox: 5 unread. Quiet until 09:00" [kpadopt5-notices]
 * @property {string} noticesLoading  What the busy card over the list says, and the counter's label while it loads [kpadopt5-notices]
 * @property {(time: string) => string} noticesQuiet  The quiet line, "Quiet until 09:00" [kpadopt5-notices]
 * @property {string} noticesEndQuiet  The button that ends the quiet time [kpadopt5-notices]
 * @property {string} noticesSinceVerb  The verb of a notice's age line, "since 2 min ago" [kpadopt5-notices]
 * @property {string} noticesMarkRead  The per-row button that marks a notice read [kpadopt5-notices]
 * @property {string} noticesMarkUnread  The per-row button that marks it unread again [kpadopt5-notices]
 * @property {string} noticesReadAll  The button that marks every notice read [kpadopt5-notices]
 * @property {(n: number) => string} noticesAllInInbox  The footer link, "All 37 in the Inbox", with the exact count [kpadopt5-notices]
 * @property {string} noticesEmpty  The empty plate's title [kpadopt5-notices]
 * @property {string} noticesEmptyText  Its sentence [kpadopt5-notices]
 * @property {string} noticesErrorTitle  The failed plate's title [kpadopt5-notices]
 * @property {string} noticesErrorText  Its sentence when the page gives no reason [kpadopt5-notices]
 * @property {string} noticesRetry  The failed plate's Retry button [kpadopt5-notices]
 * @property {string} noticesSources  The title of the per-source switches [kpadopt5-notices]
 * @property {string} noticesMuted  A muted source's state word [kpadopt5-notices]
 * @property {string} noticesNotMuted  A source that is on [kpadopt5-notices]
 * @property {(title: string) => string} noticesNew  Announced when one new notice arrived [kpadopt5-notices]
 * @property {(n: number, title: string) => string} noticesNewMany  Announced when several arrived, naming the worst [kpadopt5-notices]
 * @property {string} questionAllow  The default first answer (js/question-card.js) [kpadopt5-question-card]
 * @property {string} questionStop  The default second answer [kpadopt5-question-card]
 * @property {string} questionsLabel  The accessible name of the region that holds the open questions [kpadopt5-question-card]
 * @property {(duration: string) => string} questionLeft  How long a question still waits, "Waits 1 min 12 s more" [kpadopt5-question-card]
 * @property {string} questionTimeUp  The time line when the time ran out [kpadopt5-question-card]
 * @property {(label: string) => string} questionDefault  What happens at zero, "If nobody answers: Stop." [kpadopt5-question-card]
 * @property {(duration: string) => string} questionUrgent  Announced once when the time gets short [kpadopt5-question-card]
 * @property {string} questionNobody  The outcome of a question nobody answered, when there is no default answer [kpadopt5-question-card]
 * @property {(label: string) => string} questionNobodyDefault  The outcome when the default answer applied [kpadopt5-question-card]
 * @property {(label: string) => string} questionAnswered  The outcome of an answer that has no sentence of its own [kpadopt5-question-card]
 * @property {(why: string) => string} questionRefused  The note when the app could not send the answer, "Not sent: why. Nothing changed; answer again." [kpadopt5-question-card]
 * @property {string} questionDismiss  The button that takes a finished card away [kpadopt5-question-card]
 * @property {(count: number) => string} questionWaiting  The tab title's prefix while questions wait, "(1) waiting" [kpadopt5-question-card]
 * @property {string} transferBar  The accessible name of a transfer's bar when it has no file name (js/transfer.js) [kpadopt5-transfer]
 * @property {(done: string, total: string) => string} transferOf  The reading of a transfer with a known size, "1.2 GB of 4.1 GB" [kpadopt5-transfer]
 * @property {(done: string) => string} transferSoFar  The reading of a transfer of unknown size, "1.2 GB so far" [kpadopt5-transfer]
 * @property {string} transferSizeUnknown  The time column of a transfer whose total size is not known, beside "1.2 GB so far" [kpadopt5-r3]
 * @property {string} transferEstimating  The time left while the speed is not steady yet [kpadopt5-transfer]
 * @property {(duration: string) => string} transferLeft  The time left, "about 3 min left" [kpadopt5-transfer]
 * @property {(duration: string) => string} transferStalled  A transfer that moved no byte for a while, "stalled for 12 s" [kpadopt5-transfer]
 * @property {(rate: string) => string} transferAverage  A finished transfer's average speed, "avg 21 MB/s" [kpadopt5-transfer]
 * @property {(duration: string) => string} transferTook  A finished transfer's duration, "in 3 min 12 s" [kpadopt5-transfer]
 * @property {string} transferFailed  The first words of a failed transfer's note [kpadopt5-transfer]
 * @property {string} transferRetry  The Retry button of a failed transfer [kpadopt5-transfer]
 * @property {(name: string, percent: number) => string} transferAnnounce  What a screen reader hears at 25, 50, 75 and 100 % [kpadopt5-transfer]
 * @property {string} connOffline  The banner's first line when the connection is gone (js/connection.js) [kpadopt5-connection]
 * @property {string} connOfflineWhy  What is true meanwhile and what happens next [kpadopt5-connection]
 * @property {string} connReconnecting  The first line while a try is under way [kpadopt5-connection]
 * @property {string} connReconnectingWhy  Its second line on the first try [kpadopt5-connection]
 * @property {(n: number) => string} connAttempt  Its second line from the second try, "Attempt 2" [kpadopt5-connection]
 * @property {(duration: string) => string} connNextTry  The time to the next automatic try, "next try in 8 s" [kpadopt5-connection]
 * @property {(n: number) => string} connPending  The exact count of changes waiting, "3 changes waiting" [kpadopt5-connection]
 * @property {string} connBack  The first line when the connection is back [kpadopt5-connection]
 * @property {string} connBackWhy  Its second line with nothing waiting [kpadopt5-connection]
 * @property {(n: number) => string} connBackSending  Its second line with changes waiting, "Sending 3 changes" [kpadopt5-connection]
 * @property {string} connRefused  The first line when the server refuses while the browser is online [kpadopt5-connection]
 * @property {string} connRefusedWhy  Its second line when the app gives none [kpadopt5-connection]
 * @property {string} connRetry  The Retry now button [kpadopt5-connection]
 * @property {(duration: string) => string} refreshEvery  A refresh choice, "Every 30 s" (js/auto-refresh.js) [kpadopt5-auto-refresh]
 * @property {string} refreshPaused  The choice that stops the refreshing [kpadopt5-auto-refresh]
 * @property {string} refreshOther  The entry that lets a person type any number of seconds [kpadopt5-auto-refresh]
 * @property {string} refreshLabel  The accessible name of the refresh chooser [kpadopt5-auto-refresh]
 * @property {string} refreshNow  The button that refreshes at once [kpadopt5-auto-refresh]
 * @property {string} refreshUpdating  The freshness line's words while the app reads [kpadopt5-auto-refresh]
 * @property {(next: string) => string} refreshFailed  The note after a failed refresh, "Last refresh failed, retrying in 30 s" [kpadopt5-auto-refresh]
 * @property {string} refreshFailedPaused  The note after a failed refresh while paused, when nothing retries by itself, "Last refresh failed." [live-options-1019]
 * @property {string} refreshRetryNow  The button that retries a failed refresh at once [kpadopt5-auto-refresh]
 * @property {string} refreshCustomLabel  The label of the field for any number of seconds [kpadopt5-auto-refresh]
 * @property {string} refreshApply  The button that sets that number [kpadopt5-auto-refresh]
 * @property {string} refreshInvalid  What a number that is not a whole number of seconds says [kpadopt5-auto-refresh]
 * @property {string} paneEmpty  The detail pane's plate while nothing is picked (js/master-detail.js) [kpadopt5-master-detail]
 * @property {string} paneBack  The Back button at the top of a folded detail pane [kpadopt5-master-detail]
 * @property {string} paneLoading  What the busy card over the detail pane says [kpadopt5-master-detail]
 * @property {string} paneErrorTitle  The errored detail's first line, when the page gives none [kpadopt5-master-detail]
 * @property {string} paneErrorText  Its sentence, when the page gives none [kpadopt5-master-detail]
 * @property {string} paneRetry  The Retry button of an errored detail [kpadopt5-master-detail]
 * @property {string} busy
 * @property {string} close
 * @property {string} menu          The accessible name of a collapsed navigation's toggle
 * @property {string} closeMenu     The same toggle once the navigation is open
 * @property {string} navDisclosure  The accessible name of a mega-menu button in the bar that holds no words of its own [scope-48]
 * @property {string} sidebar       The accessible name of a hidden side navigation's toggle
 * @property {string} closeSidebar  The same toggle once that side navigation is open
 * @property {string} collapseRail  The accessible name of an empty slim-rail toggle while the rail is expanded
 * @property {string} expandRail    The same toggle while the rail is collapsed to its icons
 * @property {string} resizeSidebar The name of the resize handle on a resizable side navigation
 * @property {string} backToTop    The control that returns the reader to the top of the page
 * @property {string} previous
 * @property {string} next
 * @property {string} finish
 * @property {string} back
 * @property {(name: string) => string} removeNamed
 * @property {string} noResults
 * @property {string} oneResult
 * @property {(n: number) => string} manyResults
 * @property {string} noCommands
 * @property {string} oneCommand
 * @property {(n: number) => string} manyCommands
 * @property {string} commandPlaceholder
 * @property {string} commandsLabel
 * @property {string} shortcutsLabel
 * @property {string} paletteTrigger  The visible word on the command palette's trigger in the bar [scope-48]
 * @property {(key: string, mac: boolean) => string} paletteHotkey  The hotkey as that trigger prints it: ⌘K on a Mac, Ctrl K elsewhere [scope-48]
 * @property {string} tableSearch
 * @property {string} tableSearchLabel
 * @property {string} tableSelectAll
 * @property {(key: string) => string} tableSelectRow
 * @property {string} tableEmpty
 * @property {(n: number) => string} tableRows
 * @property {(shown: number, total: number) => string} tableRowsFiltered
 * @property {(at: number, of: number) => string} tablePage
 * @property {string} tableRegion
 * @property {string} tableSearchScope      The name of the "In" choice beside the search box
 * @property {string} tableSearchAllColumns The "In" choice's first option
 * @property {(active: number) => string} tableFilters  The filter panel's toggle, with the number of active filters
 * @property {string} tableFiltersLabel     The filter panel's group name
 * @property {string} tableActiveFilters    The list of removable filter pills
 * @property {(column: string, value: string) => string} tableFilterValue  A choice filter's pill
 * @property {(column: string, from: string, to: string) => string} tableFilterRange  A range or date filter's pill; an open end is `tableFilterOpenEnd`
 * @property {string} tableFilterOpenEnd    The open end of a range in its pill
 * @property {(column: string) => string} tableFilterFrom  The lower bound's accessible name
 * @property {(column: string) => string} tableFilterTo    The upper bound's accessible name
 * @property {(label: string) => string} tableRemoveFilter  A pill's remove button
 * @property {string} tableClearFilters
 * @property {string} toolbarShowingOnly  The word before the filters that are on, in a list toolbar's active row (js/toolbar.js) [kpadopt-controls]
 * @property {string} toolbarClearAll       The toolbar's button that turns every filter off [kpadopt-controls]
 * @property {string} keysLabel            The accessible name of a line of keyboard shortcuts (js/keys.js) [kpadopt-controls]
 * @property {string} heatLoading           What a heat grid's or matrix's cell says while its value is being read (js/heat.js) [kpadopt2-grids]
 * @property {string} heatEmpty             A heat grid or matrix with no rows [kpadopt2-grids]
 * @property {string} heatFailed            A heat grid or matrix that could not be read [kpadopt2-grids]
 * @property {string} heatLegendLess        The less end of a heat legend's value ramp (js/heat.js, setHeatLegend) [gap-tab-count]
 * @property {string} heatLegendMore        The more end of the ramp [gap-tab-count]
 * @property {string} heatLegendLabel       The name of a heat grid's legend for a screen reader [gap-tab-count]
 * @property {string} heatKeysMove          The heat legend's key line: the arrow keys [gap-tab-count]
 * @property {string} heatKeysPick          Enter or Space on a cell: pick the column [gap-tab-count]
 * @property {string} heatKeysDrop          Escape: drop the picked column [gap-tab-count]
 * @property {string} heatKeysPage          Page Up and Page Down: show earlier or later columns [gap-tab-count]
 * @property {string} matrixSelf            A matrix cell that pairs a row with itself, for a screen reader [kpadopt2-grids]
 * @property {string} matrixUnknown         A matrix cell with no answer, for a screen reader [kpadopt2-grids]
 * @property {string} tableClearSearch      The way out of the no-match state
 * @property {(active: number) => string} tableAddFilter  The "+ Add filter" button of a table in the add-filter mode, with the number of active filters
 * @property {string} tableAddFilterMenu    The name of its menu of filterable columns
 * @property {(column: string) => string} tableAddFilterItem  A column in that menu, not filtered yet
 * @property {(column: string) => string} tableAddFilterItemActive  A column in that menu that is filtered already: choosing it edits that filter
 * @property {string} tableFilterMarked     The mark beside a filtered column in the menu
 * @property {(column: string) => string} tableFilterEditor  The editor's accessible name
 * @property {(column: string) => string} tableFilterChoicesLegend  The editor's list of choices, for a screen reader
 * @property {(kind: 'range' | 'date', bound: 'from' | 'to', column: string) => string} tableFilterBound  A bound's visible label in the editor
 * @property {(column: string, values: string[]) => string} tableFilterChoicePill  A choice filter's one pill in the add-filter mode
 * @property {(column: string, from: string, to: string, kind: 'range' | 'date') => string} tableFilterSpanPill  A range or date filter's pill in the add-filter mode; an open end is left out
 * @property {(label: string) => string} tableEditFilter  A pill's own button, which reopens its editor
 * @property {string} tableFilterApply
 * @property {string} tableFilterCancel
 * @property {string} tableFilterClearAll   The add-filter mode's way out of every filter, shown from two pills
 * @property {(label: string, value: string) => string} tableFilterNotNumber  A number bound that is not a number
 * @property {(label: string, value: string) => string} tableFilterNotDate  A date bound the picker cannot read
 * @property {(from: string, to: string) => string} tableFilterBackwards  A range whose lower bound is above its upper
 * @property {string} tableDensity
 * @property {string} tableDensityComfortable
 * @property {string} tableDensityCompact
 * @property {string} tableRowsPerPage
 * @property {(from: number, to: number, count: number, total: number) => string} tableShowing  The status line: "Showing 1–25 of 60"
 * @property {(n: number) => string} tableSelected  The action bar's count
 * @property {string} tableClearSelection
 * @property {string} tableSortBy           The card layout's sort control
 * @property {string} tableSortNone         Its first option
 * @property {string} tableSortAscending
 * @property {string} tableSortDescending
 * @property {string} tableFailed
 * @property {string} tableRetry
 * @property {(column: string, direction: 'ascending' | 'descending', kind: string) => string} tableSortKey  One key of a multi-column sort in words; `kind` is text, number, date or order
 * @property {(keys: string[]) => string} tableSortedBy  The multi-sort summary, from the keys in order
 * @property {string} tableNotSorted
 * @property {string} tableSortReset
 * @property {(seconds: number) => string} tableBusyElapsed
 * @property {string} tableColumns          The column menu's button
 * @property {string} tableColumnsLabel     The column menu's name
 * @property {(column: string) => string} tableColumnLocked  A column that cannot be hidden, in the menu
 * @property {string} tableShowAllColumns
 * @property {(shown: number, total: number) => string} tableColumnsShown
 * @property {string} tableDetailsColumn    The expansion column's header, read by a screen reader only
 * @property {(key: string) => string} tableRowDetails  A row's expand button
 * @property {(group: string, count: number) => string} tableGroupRows  A group row's fold button: the group's name and how many of its rows match [J2]
 * @property {(column: string, key: string, value: string) => string} tableEdit  An editable cell's button
 * @property {(column: string, key: string) => string} tableEditField  The editor's accessible name
 * @property {(column: string, key: string) => string} tableEditing  Said when an editor opens
 * @property {(key: string, column: string, before: string, after: string) => string} tableEdited
 * @property {(key: string, column: string, value: string) => string} tableEditUndone
 * @property {string} tableEditCancelled
 * @property {string} tableEditRequired
 * @property {string} tableEditInvalid     A rejected edit with no message of its own
 * @property {string} tableGridStart        The keyboard grid's line before the first cell is focused
 * @property {(row: number, rows: number, column: string, text: string) => string} tableGridPosition  Row 0 is the header row
 * @property {string} formRequired
 * @property {string} formInvalid
 * @property {string} formSummaryOne
 * @property {(n: number) => string} formSummaryMany
 * @property {string} fieldFallbackName
 * @property {string} switchOn   The word beside a switch that is on [gap-11]
 * @property {string} switchOff  The word beside a switch that is off [gap-11]
 * @property {string} calendarOpen
 * @property {string} calendarButton
 * @property {string} dateFormatHint
 * @property {string} previousMonth
 * @property {string} nextMonth
 * @property {(month: string, year: number) => string} monthTitle
 * @property {(title: string) => string} chooseMonth  The month title's name: it opens the twelve months [scope-89]
 * @property {(year: number) => string} chooseYear    The year title's name: it opens twelve years [scope-89]
 * @property {string} previousYear
 * @property {string} nextYear
 * @property {string} previousYears  Twelve years back, in the year grid
 * @property {string} nextYears      Twelve years on, in the year grid
 * @property {(year: number) => string} monthGrid           The month grid's name, and what is said when it opens
 * @property {(from: number, to: number) => string} yearGrid The year grid's name, and what is said when it opens
 * @property {(from: number, to: number) => string} yearRange The year grid's title
 * @property {string[]} weekdays
 * @property {string[]} months
 * @property {string[]} monthsShort  The month grid's cells, where a full name does not fit [scope-89]
 * @property {(day: number, month: string, year: number) => string} dayLabel
 * @property {string} uploadZone
 * @property {(size: string) => string} uploadTooLarge
 * @property {(max: number) => string} uploadTooMany
 * @property {(max: string) => string} uploadTotalTooLarge
 * @property {(accept: string) => string} uploadWrongType
 * @property {(name: string) => string} uploadProgress
 * @property {(at: number, of: number) => string} wizardStep
 * @property {string} copy
 * @property {string} copied
 * @property {string} copyBlocked
 * @property {(value: string) => string} copiedAnnouncement
 * @property {string} copyBlockedAnnouncement
 * @property {string} undo
 * @property {string} deleted
 * @property {string} secretLabel  A secret field's name when the page gives none (js/patterns.js, `.kp-secret`) [kpadopt4]
 * @property {string} secretReveal  The button that shows a secret
 * @property {string} secretHide  The same button while the secret is shown
 * @property {string} secretHold  The button of a secret shown only while it is held (`data-kp-secret-reveal-on="hold"`)
 * @property {string} secretCopy  The button that copies a secret without showing it
 * @property {string} secretReading  The words beside the spinner while a secret is fetched
 * @property {string} secretRetry  The reveal button's words after a secret could not be read
 * @property {(seconds: number) => string} secretHidesIn  The countdown under a shown secret, "Hides in 28 s"
 * @property {string} secretPaused  The countdown's words while the reader selects or holds the focus in the value
 * @property {string} secretStaysShown  The line under a shown secret that hides only when asked (`data-kp-secret-ttl="0"`)
 * @property {(seconds: number) => string} secretShown  What a screen reader hears when a secret is shown, "Shown. Hides in 30 s."
 * @property {string} secretHidden  What it hears when the secret is hidden again
 * @property {(reason: string) => string} secretFailed  The line under a secret that could not be read
 * @property {string} secretNoSource  The reason when a secret has no value, loader or listener to read it from
 * @property {string} splitLabel
 * @property {(name: string) => string} reorderHandle
 * @property {(name: string, at: number, of: number) => string} reorderMoved
 * @property {string} tileFallbackName
 * @property {(name: string, column: number, row: number, w: number, h: number) => string} tileLabel
 * @property {(token: string) => string} contrastMissing
 * @property {(ratio: string, token: string, verdict: string) => string} contrastReport
 * @property {string} colourHue
 * @property {string} colourSaturation
 * @property {string} colourLightness
 * @property {string} contrastPasses
 * @property {string} contrastFails
 * @property {string} confirm
 * @property {string} confirmAccept
 * @property {string} confirmCancel
 * @property {string} confirmDescription
 * @property {(count: number) => string} confirmDependents  The heading over what still uses the thing a confirmation would remove, "Still in use by 2" (`data-kp-confirm-dependents`) [kpadopt4]
 * @property {string} confirmBlocked  What a confirmation that may not go ahead says when the page gives no sentence (`data-kp-confirm-blocked`) [kpadopt4]
 * @property {string} confirmClose  The one button of a confirmation that may not go ahead [kpadopt4]
 * @property {string} alarmAction  The button that closes an alarm that must be acknowledged, when the caller names none [scope-94]
 * @property {string} alarmKeepOpen  The button that stops an alarm's countdown and keeps it open [scope-94]
 * @property {string} alarmHint  The key hint beside the acknowledge button; hidden from a screen reader [scope-94]
 * @property {(seconds: number) => string} alarmCountdown  The whole seconds left under the countdown bar; hidden from a screen reader [scope-94]
 * @property {(seconds: number) => string} alarmClosesBy  Said once when an alarm that closes by itself opens [scope-94]
 * @property {(action: string) => string} alarmPressTo  Said when an alarm that must be acknowledged opens [scope-94]
 * @property {(action: string) => string} alarmKeptOpen  Said when Keep open stopped the countdown [scope-94]
 * @property {string} save
 * @property {string} mainNavigation
 * @property {string} skipToContent
 * @property {string} diagnosticsEffects  The diagnostics row that lists unknown hook values [AR44]
 * @property {string} diagnosticsEffectsNone
 * @property {string} classified  The stamp a register may print on an emphasis reveal (the dossier) [AR35]
 * @property {string} arrivalLine  The neutral boot line of an arrival, for a theme that performs one and has no words of its own in `arrivalWordsByTheme` or `arrivalLinesByTheme` [SW2, scope-84]
 * @property {Record<string, { line?: string, progress?: string, ready?: string }>} arrivalWordsByTheme  A theme's own words for the counting boot, keyed by theme [scope-84]: its line, the word before the percentage, and the word that closes it. A word the entry leaves out is the neutral one.
 * @property {Record<string, string[]>} arrivalLinesByTheme  The lines a theme's own boot shows instead of that one line, in order, keyed by theme [S49, A11]. A `{count}` in a line is replaced by a number counting up to `--kp-arrival-count` (640 by default), which is how retro's memory test reads.
 * @property {string} arrivalProgress  The neutral word before the percentage on that line
 * @property {string} arrivalReady  The neutral word that closes the boot line
 * @property {string} arrivalSkip  The button that ends the arrival at once
 * @property {string} measureLoading  A live dimension label before the first measurement lands (blueprint) [S48]
 * @property {(w: number, h: number) => string} measureBox  The size of the box the measurement frame holds (blueprint) [scope-18]
 * @property {(label: string) => string} chartPlot  A time chart's plot, for a screen reader: what it is and its keys [scope-143]
 * @property {(label: string) => string} chartSpark  A chart's 24-hour spark line, for a screen reader [scope-143]
 * @property {(label: string) => string} chartSources  The name of a chart's legend [scope-143]
 * @property {(label: string) => string} chartSource  A legend button's title: what hovering and clicking it do [scope-143]
 * @property {string} chartShowAll  The legend's way back to every source [scope-143]
 * @property {string} chartShowAllTitle  Its title [scope-143]
 * @property {string} chartHintPointer  The hint under a legend, for a pointer [scope-143]
 * @property {string} chartHintTouch  The same hint, for a finger [scope-143]
 * @property {string} chartPinned  The word in a pinned tooltip's head [scope-143]
 * @property {string} chartRelease  The pinned tooltip's ✕, named [scope-143]
 * @property {string} chartChange  The tooltip's foot: what the ▲/▼ column is [scope-143]
 * @property {string} chartOpen  An event's link in a pinned tooltip [scope-143]
 * @property {(label: string, time: string) => string} chartMark  An event marker's title [scope-143]
 * @property {(from: string, to: string) => string} chartZoomed  The zoom chip, before its Reset, from its two ends; used only when a consumer overrides it and not `chartZoomedSpan` [scope-143]
 * @property {(span: string) => string} chartZoomedSpan  The zoom chip, before its Reset: `span` is the zoom as the chart's `range` time style prints it [scope-143]
 * @property {string} chartReset  The zoom chip's button [scope-143]
 * @property {string} chartResetTitle  Its title [scope-143]
 * @property {string} chartUp  A rise, as the readout says it [scope-143]
 * @property {string} chartDown  A fall, as the readout says it [scope-143]
 * @property {string} chartSame  No change, as the readout says it [scope-143]
 * @property {string} chartNow  A spark line's value when no time is under the crosshair [scope-143]
 * @property {string} chartEmpty  A time chart with no readings in its window, in the plot at its height [scope-143]
 * @property {string} chartRetry  The Retry under a failed chart's sentence [t56-statefix-1018]
 * @property {string} chartTrendFailed  The failed plate's heading of a key figure's trend when the page gives no sentence [kyu-plates-1019]
 * @property {string} progressFailed  The failed plate's heading of a progress bar when the page gives no title [kyu-plates-1019]
 * @property {string} chartOnePoint  Under a chart whose every source has one reading (with `onePointNote`) [scope-143]
 * @property {string} chartLoading  A loading chart (`data-kp-chart-loading`), for a screen reader [scope-143]
 * @property {string} chartPinnedOutside  A pinned tooltip's head once a live update moved the window past the pin [scope-143]
 * @property {string} menuLoading  A menu button's menu while its entries load: one disabled entry (js/menu-button.js) [scope-143]
 * @property {string} menuEmpty  The title of a menu button with no entries, which stays and does nothing (`data-kp-menu-empty="disable"`) [scope-143]
 * @property {string} navMore  The word on the button a folding bar puts its overflowing links behind (`data-kp-nav-fold`, js/nav-fold.js) [kpadopt-nav]
 * @property {string} navMoreMenu  The name of that button's menu, for a screen reader [kpadopt-nav]
 * @property {string} navMoreAlert  Said after the button's name when a link behind it carries a counter [kpadopt-nav]
 * @property {string} navMoreUnavailable  Under a folded link that cannot be followed now (`aria-disabled`, `disabled`), as its reason [kpadopt-nav]
 * @property {string} tourNext  The tour card's button to the next step (js/tour.js) [scope-143]
 * @property {string} tourNextTitle  Its title [scope-143]
 * @property {string} tourBack  The tour card's button to the step before [scope-143]
 * @property {string} tourBackTitle  Its title [scope-143]
 * @property {string} tourDone  The next button on the last step, which ends the tour [scope-143]
 * @property {string} tourDoneTitle  Its title [scope-143]
 * @property {string} tourSkip  The tour card's button that ends the tour at once [scope-143]
 * @property {string} tourSkipTitle  Its title [scope-143]
 * @property {(n: number, of: number) => string} tourCount  The tour card's count, "1 of 5"; `of` counts only the steps whose part is on the page [scope-143]
 * @property {string} chartToday  A key figure's trend axis: the word after the first point's clock when it is today ("07:00 today") [scope-143]
 * @property {string} chartYesterday  The same when it is yesterday ("14:40 yesterday") [scope-143]
 * @property {string} chartTrendKeys  A key figure's trend, for a screen reader: its keys [scope-143]
 * @property {string} meterUsed  A meter's share, in the words a screen reader hears, when the page names it nothing else ("62% used") [scope-143]
 * @property {string} meterNotMeasured  A meter with no share, for a screen reader [scope-143]
 * @property {string} meterMeasuring  A loading meter, for a screen reader [scope-143]
 * @property {string} [loading]  The calendar's busy text when the page gives none (`Loading…`)
 * @property {string} loadingWord  The word a busy picture spells on screen, deciphered letter by letter (cyberpunk's skeletons, busy bars and meters); `setStrings()` hands it to the stylesheet [scope-143]
 * @property {string} calendarNav  The name of a month heatmap's row of month buttons (js/calendar.js) [scope-143]
 * @property {string} calendarPrev  A month heatmap's button to the month before; its name and title are `previousMonth` [scope-143]
 * @property {string} calendarNext  Its button to the month after; its name and title are `nextMonth` [scope-143]
 * @property {string} calendarToday  Its button back to today's month, with today picked [scope-143]
 * @property {string} calendarTodayTitle  That button's title [scope-143]
 * @property {(date: string, label: string) => string} calendarDay  A day's name and title: the date (dd/mm/yyyy) and what the page says about it [scope-143]
 * @property {string} calendarFuture  What a day still to come is, when the page says nothing about it [scope-143]
 * @property {string} calendarLoading  What every day is while the calendar loads [scope-143]
 * @property {string} calendarUnknown  What a past day is when the page says nothing about it [scope-143]
 * @property {string} weekGroup  The name of a week agenda when the page gives none (js/week.js)
 * @property {string} weekPrev  The week agenda's button to the week before
 * @property {string} weekNext  Its button to the week after
 * @property {string} weekToday  Its button back to the current week; also the word under today's date
 * @property {string} weekNow  The now-line's name
 * @property {(time: string) => string} weekNowAt  The now-line's name with the time, "now, 14:32"
 * @property {(n: number) => string} weekMore  The overflow pill's text, "+3"
 * @property {(n: number, day: string) => string} weekMoreLabel  The overflow pill's name, "3 more runs on Tue 17/10"
 * @property {string} weekNextRun  What a next run is called in a pill's name
 * @property {string} weekOff  What a switched-off item is called in a pill's name
 * @property {string} weekSkipped  What a skipped item is called in a pill's name
 * @property {string} weekReference  What a reference item (the host's own round) is called in a pill's name
 * @property {string} weekEmpty  The sentence of a week with nothing in it
 * @property {string} weekLoading  The sentence of a week being read
 * @property {string} ganttGroup  The name of a run timeline when the page gives none (js/gantt.js)
 * @property {string} ganttRange  The name of the range buttons
 * @property {(label: string) => string} ganttRangeTitle  A range button's title, "Show the last 6 hours"
 * @property {string} ganttNow  The now-line's name
 * @property {string} ganttSelection  The selected range's chip label
 * @property {string} ganttClear  The button that lifts the selected range
 * @property {string} ganttClearTitle  Its title
 * @property {(from: string, to: string) => string} ganttSelected  The chip's text, "Selected 14:00 to 16:30"
 * @property {string} ganttEmpty  The sentence of a timeline with no run in the window
 * @property {string} ganttLoading  The sentence of a timeline being read
 * @property {string} ganttQueued  A queued run's state word
 * @property {string} ganttRunning  A running run's state word
 * @property {string} ganttOk  A finished run's state word
 * @property {string} ganttFailed  A failed run's state word
 * @property {(label: string, state: string) => string} ganttAnnounce  What the timeline's status region says when a run changes state, "Backup: failed"
 * @property {(label: string, when: string, state: string) => string} ganttBar  A bar's name, "Backup, 02:00 to 02:14, failed"
 * @property {string} ganttLive  The button that makes the window follow now
 * @property {string} ganttLiveTitle  Its title
 * @property {string} ganttZoom  The button that makes the selected range the window
 * @property {string} ganttZoomTitle  Its title
 * @property {(seconds: number) => string} ganttDuration  A length of time as a person reads it, "14 min", "1 h 5 min", "45 s"
 * @property {string} ganttHint  The one-line hint under the timeline on how to read and use it
 * @property {string} ganttHintOpen  The same hint of a timeline without its range toolbar (no brush): only opening a bar [gaps-time-1019]
 * @property {string} ganttAll  The range button that fits the window to everything on the timeline (`all` in `data-kp-gantt-ranges`) [kpadopt4]
 * @property {string} ganttAllTitle  Its title
 * @property {(label: string, when: string) => string} ganttPoint  A point event's name, "Applied, 02/10/2026" [kpadopt4]
 * @property {string} ganttPlanned  The word after a planned point's name, still to come
 * @property {(count: number, labels: string) => string} ganttCluster  The name of marks too close to tell apart, "3 events: Applied, Mail, Call"
 * @property {(more: number) => string} ganttClusterMore  The count on such a mark, "+2"
 * @property {string} ganttPointsHint  The hint under a timeline of point events
 * @property {string} pipelineLoading  The words on a pipeline's loading card (js/pipeline.js) [kpadopt4]
 * @property {string} pipelineEmpty  The sentence over a pipeline with no cards, when the page gives none
 * @property {(reason: string) => string} pipelineFailed  The sentence over a pipeline that could not be read
 * @property {string} pipelineRetry  Its Try again button
 * @property {string} pipelineDanger  The word on a failing lane's name plate (`danger: true`), so the colour is never the only carrier [r1r2-1019]
 * @property {string} pipelineNone  The word in a lane with no cards
 * @property {(more: number) => string} pipelineMore  The button under a lane's first cards, "+4 more"
 * @property {string} pipelineLess  The same button while the lane shows every card
 * @property {(percent: number) => string} pipelineShare  A lane's share of all cards, "25 % of all"
 * @property {string} uptimeLoading  The uptime strip's busy card and its summary while the history is read (js/uptime.js) [kpadopt4-ideas]
 * @property {string} uptimeEmpty  Its summary when no period has data yet [kpadopt4-ideas]
 * @property {string} uptimeError  Its summary when the history could not be read and the page gave no sentence [kpadopt4-ideas]
 * @property {(percent: string) => string} uptimePercent  The window's share up, from "99.4" to "99.4%" [kpadopt4-ideas]
 * @property {(n: number) => string} uptimeIncidents  The window's incident count, exact: "1 incident", "12 incidents" [kpadopt4-ideas]
 * @property {(percent: string, incidents: number) => string} uptimeSummary  The summary as one sentence, for a screen reader [kpadopt4-ideas]
 * @property {(label: string, summary: string) => string} uptimeTrack  The strip's name: its title and its summary [kpadopt4-ideas]
 * @property {(state: string) => string} uptimeStateWord  A period's state as a word: up, degraded, down, no data [kpadopt4-ideas]
 * @property {(label: string, state: string, percent: string | null, incidents: number | null) => string} uptimeCell  A cell's name and hover card: its period, its state and its figures [kpadopt4-ideas]
 * @property {(shown: number, total: number) => string} uptimeShown  Under a strip too narrow for every cell: "55 of 90 shown" [kpadopt4-ideas]
 * @property {(percent: string | null, incidents: number | null) => string} uptimeInfoFigures  A period's figures in its quick info: "99.4% up · 1 incident" [kpadopt4 r2]
 * @property {(label: string) => string} uptimeOpen  The link in a period's quick info to the period's own page: "Open 14/10/2026" [kpadopt4 r2]
 * @property {(name: string) => string} typeConfirmLabel  The typed confirmation's label; the name is set in as code where this sentence puts it (js/components.js) [kpadopt4-ideas]
 * @property {string} typeConfirmRuleExact  Its rule when capitals count [kpadopt4-ideas]
 * @property {string} typeConfirmRuleAnyCase  Its rule when capitals do not count (`data-kp-confirm-case="any"`) [kpadopt4-ideas]
 * @property {string} typeConfirmMatched  The badge once the typed name matches [kpadopt4-ideas]
 * @property {string} typeConfirmMismatch  The line while what is typed does not match yet [kpadopt4-ideas]
 * @property {string} typeConfirmLoses  The heading over what the action will delete [kpadopt4-ideas]
 * @property {(shell: string) => string} commandShell  A command block's terminal by its key: WSL, PowerShell, cmd, Garuda, bash; another key as written (js/command.js) [kpadopt4-ideas]
 * @property {string} commandAdmin  The badge of a Windows command that needs a terminal opened as administrator [kpadopt4-ideas]
 * @property {string} commandAdminTitle  Its tooltip [kpadopt4-ideas]
 * @property {string} commandSudo  The badge of a Linux command that runs with root rights [kpadopt4-ideas]
 * @property {string} commandSudoTitle  Its tooltip [kpadopt4-ideas]
 * @property {(where: string) => string} commandRunsOn  The note under a command that says where it runs [kpadopt4-ideas]
 * @property {string} timerStart  A timer's Start button (js/countdown.js, js/timer-disc.js) [kpadopt4-ideas]
 * @property {string} timerPause  The same button while it runs [kpadopt4-ideas]
 * @property {string} timerResume  The same button while it is paused [kpadopt4-ideas]
 * @property {string} timerReset  A timer's Reset button [kpadopt4-ideas]
 * @property {(seconds: number) => string} timerAddMinute  The countdown's button that adds time: "+1 min" [kpadopt4-ideas]
 * @property {(seconds: number) => string} timerAddMinuteTitle  Its tooltip: "Add 1 min" [kpadopt4-ideas]
 * @property {string} timerPaused  The line under a paused timer, and what a screen reader hears [kpadopt4-ideas]
 * @property {string} timerEnded  The line under a timer whose time is up [kpadopt4-ideas]
 * @property {(name: string) => string} timerEndedNamed  What a screen reader hears when a named timer ends [kpadopt4-ideas]
 * @property {string} timerStarted  What a screen reader hears when a timer starts [kpadopt4-ideas]
 * @property {string} timerResumed  What a screen reader hears when it goes on after a pause [kpadopt4-ideas]
 * @property {(duration: string) => string} timerLeft  The countdown's time left for a screen reader: "25 min left" [kpadopt4-ideas]
 * @property {(seconds: number) => string} timerDuration  A timer's length in words: "25 min", "1 h 30 min", "45 s" [kpadopt4-ideas]
 * @property {(left: string, total: string) => string} timerDiscLeft  The visual timer's face for a screen reader: "12 min left of 25 min" [kpadopt4-ideas]
 * @property {string} timerUnderMinute  The time left on the face in its last minute [kpadopt4-ideas]
 * @property {(name: string, duration: string) => string} timerDiscCaption  The still line under the disc: "Focus · 25 min" [kpadopt4-ideas]
 * @property {(when: string) => string} timerUntil  The still line of a timer that counts down to a moment: "until 14:30" [gaps-time-1019]
 * @property {string} rangePresets  The range picker's preset group, for a screen reader (js/range.js) [kpadopt4-ideas]
 * @property {(key: string) => string} rangePresetName  A preset in words: "Last 24 hours" [kpadopt4-ideas]
 * @property {(key: string) => string} rangePresetShort  A preset on its segment: "24h" [kpadopt4-ideas]
 * @property {string} rangeCustom  The button that opens the custom range [kpadopt4-ideas]
 * @property {string} rangeCustomTitle  The custom panel's title [kpadopt4-ideas]
 * @property {string} rangeCustomName  A custom window's name in the line under the picker [kpadopt4-ideas]
 * @property {string} rangeFrom  The start's label [kpadopt4-ideas]
 * @property {string} rangeTo  The end's label [kpadopt4-ideas]
 * @property {(bound: string) => string} rangeDateLabel  A date field's name: "From, date (dd/mm/yyyy)" [kpadopt4-ideas]
 * @property {(bound: string) => string} rangeTimeLabel  A time field's name [kpadopt4-ideas]
 * @property {string} rangeTimeHint  The time field's placeholder [kpadopt4-ideas]
 * @property {string} rangeApply  The custom panel's Apply [kpadopt4-ideas]
 * @property {string} rangeCancel  The custom panel's Cancel [kpadopt4-ideas]
 * @property {(name: string, from: string, to: string) => string} rangeLabel  The line under the picker: "Last 24 hours · 15/10/2026 14:05 – 16/10/2026 14:05" [kpadopt4-ideas]
 * @property {(bound: string) => string} rangeErrorDate  A date that is not a whole dd/mm/yyyy [kpadopt4-ideas]
 * @property {(bound: string) => string} rangeErrorTime  A time that is not a time of day [kpadopt4-ideas]
 * @property {string} rangeErrorOrder  A window whose end does not come after its start [kpadopt4-ideas]
 * @property {string} rangeErrorFuture  A window that ends in the future [kpadopt4-ideas]
 * @property {(days: number) => string} rangeErrorSpan  A window longer than the picker allows [kpadopt4-ideas]
 * @property {string} weekdaysGroup  The weekday picker's name when the page gives none (js/weekdays.js) [kpadopt5-time]
 * @property {string[]} weekdaysShort  The short day names, Monday first: "Mon" [kpadopt5-time]
 * @property {string[]} weekdaysNarrow  The one-letter day names, Monday first: "M" [kpadopt5-time]
 * @property {string[]} weekdaysLong  The full day names, Monday first, a chip's hint and name [kpadopt5-time]
 * @property {string} weekdaysLast  The month picker's chip for the last day [kpadopt5-time]
 * @property {string} weekdaysLastDay  The last day of the month in words [kpadopt5-time]
 * @property {string} weekdaysAnd  The word that joins the last two days of a list: "Mon and Thu" [kpadopt5-time]
 * @property {string} weekdaysPresets  The quick picks' group name for a screen reader [kpadopt5-time]
 * @property {string} weekdaysPresetWeekdays  The quick pick for Monday to Friday [kpadopt5-time]
 * @property {string} weekdaysPresetWeekend  The quick pick for Saturday and Sunday [kpadopt5-time]
 * @property {string} weekdaysPresetEvery  The quick pick for all seven days [kpadopt5-time]
 * @property {string} weekdaysNone  The line while no day is picked [kpadopt5-time]
 * @property {string} weekdaysEvery  The line for all seven days [kpadopt5-time]
 * @property {string} weekdaysWeekdays  The line for Monday to Friday [kpadopt5-time]
 * @property {string} weekdaysWeekend  The line for Saturday and Sunday [kpadopt5-time]
 * @property {(n: number) => string} weekdaysOrdinal  A day of the month in words: "1st", "22nd" [kpadopt5-time]
 * @property {(list: string) => string} weekdaysOnThe  The days of a month in a sentence: "On the 1st and 15th" [kpadopt5-time]
 * @property {(n: number) => string} weekdaysMin  What a picker below its least says: "Pick at least one day" [kpadopt5-time]
 * @property {string} runsTitle  The heading over a next-times list (js/runs.js) [kpadopt5-time]
 * @property {string} runsNow  A run that is due now, where "in 2 days" would stand [kpadopt5-time]
 * @property {(distance: string) => string} runsIn  How far away a run is: "in 2 days" [kpadopt5-time]
 * @property {(hours: number) => string} runsClockChange  The mark on a run after the clock was turned, positive hours forward: "Clock goes back 1 h" [kpadopt5-time]
 * @property {(n: number) => string} runsMore  The button that shows the times past the count: "Show 10 more" [kpadopt5-time]
 * @property {string} runsFewer  The same button once they are shown [kpadopt5-time]
 * @property {string} runsEmpty  An empty list's sentence when the page gives none [kpadopt5-time]
 * @property {string} runsFailed  A failed list's first line [kpadopt5-time]
 * @property {string} runsRetry  The failed list's button [kpadopt5-time]
 * @property {string} runsLoading  What the busy card says while the times are worked out [kpadopt5-time]
 * @property {(moment: string) => string} runsNext  What the status region says when the first run changes [kpadopt5-time]
 * @property {string} snoozeTrigger  The snooze picker's button while nothing is snoozed (js/snooze.js) [kpadopt5-time]
 * @property {string} snoozeChange  The same button while a snooze is in force [kpadopt5-time]
 * @property {string} snoozeChoicesLabel  The chip row's name for a screen reader [kpadopt5-time]
 * @property {(key: string) => string} snoozeIn  A length as a choice: "In 4 hours" [kpadopt5-time]
 * @property {(time: string) => string} snoozeTonight  "This evening 18:00" [kpadopt5-time]
 * @property {(time: string) => string} snoozeTomorrow  "Tomorrow 09:00" [kpadopt5-time]
 * @property {(time: string) => string} snoozeMonday  "Monday 09:00" [kpadopt5-time]
 * @property {(time: string) => string} snoozeNextWeek  "Next week 09:00" [kpadopt5-time]
 * @property {string} snoozeLast  The last custom moment, offered at the top [kpadopt5-time]
 * @property {string} snoozePick  The choice that opens the date and time panel [kpadopt5-time]
 * @property {string} snoozePickHint  What stands under that choice [kpadopt5-time]
 * @property {string} snoozePickTitle  The panel's title [kpadopt5-time]
 * @property {string} snoozeDateLabel  The panel's date field name [kpadopt5-time]
 * @property {string} snoozeTimeLabel  The panel's time field name [kpadopt5-time]
 * @property {string} snoozeApply  The panel's confirm button [kpadopt5-time]
 * @property {string} snoozeCancel  The panel's cancel button [kpadopt5-time]
 * @property {(moment: string) => string} snoozeUntil  The line while snoozed: "Snoozed until Mon 20/10/2026 09:00 · in 2 days" [kpadopt5-time]
 * @property {string} snoozeNone  The line while nothing is snoozed [kpadopt5-time]
 * @property {string} snoozeEnd  The button that ends a snooze [kpadopt5-time]
 * @property {string} snoozeSaving  Why the choices are off while the page saves [kpadopt5-time]
 * @property {string} snoozeErrorDate  A date that is not a whole dd/mm/yyyy [kpadopt5-time]
 * @property {string} snoozeErrorTime  A time that is not a time of day [kpadopt5-time]
 * @property {string} snoozeErrorPast  A moment that has passed [kpadopt5-time]
 * @property {(days: number) => string} snoozeErrorMax  A moment past the longest a reminder may be put off [kpadopt5-time]
 * @property {string} datedGroup  A dated option picker's name when the page gives none (js/dated-options.js) [kpadopt5-time]
 * @property {string} datedLatest  The badge on the newest option [kpadopt5-time]
 * @property {string} datedJustNow  An option made under a minute ago [kpadopt5-time]
 * @property {(distance: string) => string} datedAgo  How long ago an option was made: "3 days ago" [kpadopt5-time]
 * @property {string} datedSearch  The search field's placeholder, for a long list [kpadopt5-time]
 * @property {(name: string) => string} datedChosen  What the status region says when an option is chosen [kpadopt5-time]
 * @property {string} datedEmpty  An empty picker's sentence when the page gives none [kpadopt5-time]
 * @property {string} datedFailed  A failed picker's first line [kpadopt5-time]
 * @property {string} datedRetry  The failed picker's button [kpadopt5-time]
 * @property {string} datedLoading  What the busy card says while the options load [kpadopt5-time]
 * @property {(label: string) => string} sentenceEmpty  What a sentence's summary says for a field with nothing in it: "(At)" (js/sentence.js) [kpadopt5-time]
 * @property {(field: string, why: string) => string} sentenceWhy  The line under a sentence that names the field that is wrong: "At: Type a time of day." [kpadopt5-time]
 * @property {string} sentenceField  The name of a control that has none and no words before it [kpadopt5-time]
 * @property {string} sliderGroup  A slider's name when the page gives none (js/slider.js) [kpadopt5-time]
 * @property {(label: string) => string} sliderMin  The lower thumb's and box's name: "Temperature, minimum" [kpadopt5-time]
 * @property {(label: string) => string} sliderMax  The upper thumb's and box's name: "Temperature, maximum" [kpadopt5-time]
 * @property {(label: string) => string} sliderValue  The value box's name: "Temperature, value" [kpadopt5-time]
 * @property {(min: string, max: string, unit: string) => string} sliderInvalid  The line under a slider for a typed value that is not a number in range [kpadopt5-time]
 * @property {(gap: string, unit: string) => string} sliderGap  The line for a typed value that would bring the two thumbs too close [kpadopt5-time]
 * @property {(n: number, text: string) => string} autosizeChars  The counter of a growing text box: "1 204 characters" (js/autosize.js) [kpadopt6-media]
 * @property {(n: number, text: string) => string} autosizeLines  Its line count: "12 lines" [kpadopt6-media]
 * @property {(chars: number, charsText: string, lines: number, linesText: string) => string} autosizeBoth  Both counts in one line [kpadopt6-media]
 * @property {(text: string, maxText: string) => string} autosizeOf  A count against the limit: "1 204 of 4 000 characters" [kpadopt6-media]
 * @property {(maxText: string) => string} autosizeFull  Said politely when the limit is reached [kpadopt6-media]
 * @property {(charsText: string, maxText: string) => string} autosizeNear  Said politely when 90 % of the limit is used [kpadopt6-media]
 * @property {string} autosizeSubmit  The sheet's words for Ctrl+Enter submitting the form [kpadopt6-media]
 * @property {string} autosizeGroup  The sheet's heading for that key [kpadopt6-media]
 * @property {string} autosizeLoading  The busy card's words while a box waits for its text [kpadopt6-media]
 * @property {string} autosizeFailed  The line under a box whose text could not be saved, when the page gives none [kpadopt6-media]
 * @property {(name: string) => string} anchorCopyLink  The accessible name of a link mark: "Copy link to Dead letter 4f2a" (js/anchors.js) [kpadopt6-media]
 * @property {string} anchorCopied  The word on the mark after a copy [kpadopt6-media]
 * @property {string} anchorFailed  The word on the mark when the clipboard refused [kpadopt6-media]
 * @property {(name: string) => string} anchorCopiedAnnounce  Said politely after a copy [kpadopt6-media]
 * @property {string} anchorFailedAnnounce  Said politely when the clipboard refused [kpadopt6-media]
 * @property {string} anchorMissing  The notice for an address that points at nothing here any more [kpadopt6-media]
 * @property {string} anchorShowAll  The notice's button that clears the address [kpadopt6-media]
 * @property {string} anchorKeyCopy  The sheet's words for y then l on a row [kpadopt6-media]
 * @property {string} anchorKeyGroup  The sheet's heading for the anchor keys [kpadopt6-media]
 * @property {string} printLoading  What a list that is still loading prints instead of a blank sheet (js/print.js) [kpadopt6-media]
 * @property {(moment: string) => string} printStamp  The foot of a printed sheet: "Printed 17/10/2026 14:05" [kpadopt6-media]
 * @property {string} viewerOpenNew  The viewer's "Open in a new tab" button (js/viewer.js) [kpadopt6-media]
 * @property {string} viewerDownload  The viewer's Download button [kpadopt6-media]
 * @property {string} viewerPrint  The viewer's Print button [kpadopt6-media]
 * @property {string} viewerCopyLink  The viewer's Copy link button [kpadopt6-media]
 * @property {string} viewerNoFile  The reason a viewer button is off when there is no file [kpadopt6-media]
 * @property {string} viewerLoading  The busy card's words while a file opens [kpadopt6-media]
 * @property {string} viewerFailedTitle  The failed stage's heading [kpadopt6-media]
 * @property {string} viewerFailedText  The failed stage's sentence when the server gave none [kpadopt6-media]
 * @property {string} viewerUnsupportedTitle  The heading of the stage that cannot show this kind of file here [kpadopt6-media]
 * @property {string} viewerUnsupportedText  Its sentence [kpadopt6-media]
 * @property {string} viewerRetry  The failed stage's Retry [kpadopt6-media]
 * @property {(index: number, total: number) => string} viewerPosition  The exact position in a set: "4 of 37" [kpadopt6-media]
 * @property {string} previewKeyGroup  The sheet's heading for the preview keys (js/docpreview.js) [kpadopt6-media]
 * @property {string} previewKeyOpen  The sheet's words for o [kpadopt6-media]
 * @property {string} previewKeyDownload  The sheet's words for d [kpadopt6-media]
 * @property {string} previewKeyPrint  The sheet's words for p [kpadopt6-media]
 * @property {string} previewKeyPrevious  The sheet's words for Left in a group of documents [kpadopt6-media]
 * @property {string} previewKeyNext  The sheet's words for Right in a group of documents [kpadopt6-media]
 * @property {string} previewTimeout  The sentence when a file takes too long to open [kpadopt6-media]
 * @property {string} previewGone  The sentence when the server says the file is gone [kpadopt6-media]
 * @property {(status: number) => string} previewServer  The sentence when the server answered with an error [kpadopt6-media]
 * @property {string} previewNoPdf  The sentence when this browser cannot show a PDF here [kpadopt6-media]
 * @property {string} previewShownByBrowser  The line under a PDF's title: how it is shown [kpadopt6-media]
 * @property {string} previewImage  The sentence when a picture would not load [kpadopt6-media]
 * @property {string} previewNoFile  The reason an opener is off when there is no file yet [kpadopt6-media]
 * @property {string} docKindImage  The facts word of a picture file [kpadopt6-media]
 * @property {string} docKindText  The facts word of a text file [kpadopt6-media]
 * @property {string} docMissing  The name of a document glyph whose file is missing [kpadopt6-media]
 * @property {(kind: string) => string} docKindName  The name of a document glyph: "PDF document" [kpadopt6-media]
 * @property {string} galleryContinue  The button that opens the picture last looked at (js/gallery.js) [kpadopt6-media]
 * @property {string} galleryFailed  The failed gallery's heading [kpadopt6-media]
 * @property {string} galleryFailedText  Its sentence [kpadopt6-media]
 * @property {string} galleryEmpty  The empty gallery's heading [kpadopt6-media]
 * @property {string} galleryEmptyText  Its sentence [kpadopt6-media]
 * @property {string} galleryEmptyAdd  Its sentence when pictures can be added [kpadopt6-media]
 * @property {string} galleryRetry  The Retry of a failed gallery or picture [kpadopt6-media]
 * @property {string} galleryAdd  The add tile's words [kpadopt6-media]
 * @property {string} galleryLoading  The busy card's words while the gallery loads [kpadopt6-media]
 * @property {string} galleryPicture  What a picture without a name is called [kpadopt6-media]
 * @property {(name: string) => string} galleryOpenPicture  The name of a tile that opens a picture [kpadopt6-media]
 * @property {(name: string) => string} galleryOpenVideo  The name of a tile that opens a video [kpadopt6-media]
 * @property {string} galleryPictureFailed  The heading of a picture that would not load [kpadopt6-media]
 * @property {string} galleryPictureFailedText  Its sentence in the lightbox [kpadopt6-media]
 * @property {(name: string) => string} galleryRetryPicture  The accessible name of a picture's Retry [kpadopt6-media]
 * @property {string} galleryPrevious  The lightbox's previous button [kpadopt6-media]
 * @property {string} galleryNext  The lightbox's next button [kpadopt6-media]
 * @property {string} galleryFirst  The reason previous is off at the first picture [kpadopt6-media]
 * @property {string} galleryLast  The reason next is off at the last picture [kpadopt6-media]
 * @property {string} galleryKeyGroup  The sheet's heading for the lightbox keys [kpadopt6-media]
 * @property {string} galleryKeyPrevious  The sheet's words for Left [kpadopt6-media]
 * @property {string} galleryKeyNext  The sheet's words for Right [kpadopt6-media]
 * @property {string} galleryKeyFirst  The sheet's words for Home [kpadopt6-media]
 * @property {string} galleryKeyLast  The sheet's words for End [kpadopt6-media]
 * @property {string} galleryKeyZoom  The sheet's words for z [kpadopt6-media]
 * @property {string} galleryKeyZoomIn  The sheet's words for + [kpadopt6-media]
 * @property {string} galleryKeyZoomOut  The sheet's words for - [kpadopt6-media]
 * @property {string} galleryKeyFull  The sheet's words for f [kpadopt6-media]
 * @property {string} usesLabel  A uses map's name when the page gives none (js/uses-map.js) [kpadopt6-media]
 * @property {string} usesSources  The sources column's name [kpadopt6-media]
 * @property {string} usesUsers  The users column's name [kpadopt6-media]
 * @property {string} usesFailed  The failed map's heading [kpadopt6-media]
 * @property {string} usesFailedText  Its sentence [kpadopt6-media]
 * @property {string} usesEmpty  The empty map's heading [kpadopt6-media]
 * @property {string} usesEmptyText  Its sentence [kpadopt6-media]
 * @property {string} stateRetry  The Retry of every failed data area drawn with the state plate (heat, matrix, timeline, week, graph, table) [state-plates-1019]
 * @property {string} stateFailedText  The sentence under a failed data area's heading when the page gives none [state-plates-1019]
 * @property {string} graphEmpty  The empty network's heading when the page gives no words [state-plates-1019]
 * @property {string} graphFailed  The failed network's heading when the page gives no words [state-plates-1019]
 * @property {string} usesRetry  The failed map's Retry [kpadopt6-media]
 * @property {string} usesLoading  The busy card's words while the map loads [kpadopt6-media]
 * @property {string} usesNotUsed  What a source nothing uses says [kpadopt6-media]
 * @property {(n: number) => string} usesCount  The exact number of uses on a source: "Used by 2" [kpadopt6-media]
 * @property {(label: string) => string} usesOpen  The name of a node's link [kpadopt6-media]
 * @property {(label: string, n: number, names: string) => string} usesNodeSource  A source in words: "CV Developer NL, used by 2: Deloitte, Takto" [kpadopt6-media]
 * @property {(label: string) => string} usesNodeUnused  A source nothing uses, in words [kpadopt6-media]
 * @property {(label: string, names: string) => string} usesNodeUser  A user in words: "Deloitte, uses: CV Developer NL" [kpadopt6-media]
 * @property {(label: string) => string} usesNodeNothing  A user that uses nothing, in words [kpadopt6-media]
 * @property {string} unsavedGroup  The unsaved-changes bar's name for a screen reader (js/unsaved.js) [kpadopt5-time]
 * @property {string} unsavedNone  The line while nothing is changed [kpadopt5-time]
 * @property {(n: number) => string} unsavedCount  The chip: "2 changes" [kpadopt5-time]
 * @property {(n: number) => string} unsavedSave  The Save button with its count: "Save 2 changes" [kpadopt5-time]
 * @property {string} unsavedSaveNone  The Save button while nothing is changed [kpadopt5-time]
 * @property {string} unsavedDiscard  The Discard button [kpadopt5-time]
 * @property {string} unsavedMore  The menu that holds Discard on a narrow bar [kpadopt5-time]
 * @property {string} unsavedDiscardChanges  Its Discard entry [kpadopt5-time]
 * @property {string} unsavedDiscardHint  What it does [kpadopt5-time]
 * @property {string} unsavedShowChanges  Its entry that opens the list [kpadopt5-time]
 * @property {string} unsavedShowHint  What it does [kpadopt5-time]
 * @property {string} unsavedNothingToSave  Why Save and Discard are off while nothing is changed [kpadopt5-time]
 * @property {string} unsavedSavingNow  Why they are off while saving [kpadopt5-time]
 * @property {string} unsavedListTitle  The list of changes' name [kpadopt5-time]
 * @property {string} unsavedGoFirst  The button that takes you to the first changed row [kpadopt5-time]
 * @property {(key: string) => string} unsavedGoTo  A change's name as a link to its row [kpadopt5-time]
 * @property {string} unsavedRevert  A change's own revert button [kpadopt5-time]
 * @property {(key: string) => string} unsavedRevertOne  That button's accessible name [kpadopt5-time]
 * @property {string} unsavedUnnamed  A change whose row has no name [kpadopt5-time]
 * @property {string} unsavedSecret  A secret's value in the list [kpadopt5-time]
 * @property {string} unsavedOn  A switch that is on, in the list [kpadopt5-time]
 * @property {string} unsavedOff  A switch that is off, in the list [kpadopt5-time]
 * @property {string} unsavedEmpty  An empty value, in the list [kpadopt5-time]
 * @property {(n: number) => string} unsavedDropped  What the toast says after Discard: "Dropped 2 changes" [kpadopt5-time]
 * @property {string} unsavedUndo  Its Undo button [kpadopt5-time]
 * @property {(n: number) => string} unsavedRestoredUndo  What is said after Undo [kpadopt5-time]
 * @property {(n: number) => string} unsavedRestored  The toast when kept changes come back after a reload [kpadopt5-time]
 * @property {(n: number) => string} unsavedSavedAll  What is said when every change was saved [kpadopt5-time]
 * @property {(saved: number, refused: number) => string} unsavedPartly  The alert's first line when some were refused: "1 saved, 1 refused" [kpadopt5-time]
 * @property {(n: number) => string} unsavedRefused  The alert's first line when none was saved [kpadopt5-time]
 * @property {(key: string, why: string) => string} unsavedWhy  One refused change and its reason [kpadopt5-time]
 * @property {string} unsavedDismiss  The alert's close [kpadopt5-time]
 * @property {string} unsavedLeaveTitle  The question on following a link with unsaved changes [kpadopt5-time]
 * @property {(n: number) => string} unsavedLeaveText  Its sentence [kpadopt5-time]
 * @property {string} unsavedLeaveAccept  Its leave button [kpadopt5-time]
 * @property {string} unsavedLeaveStay  Its stay button [kpadopt5-time]
 * @property {string} compareBefore  The compare slider's label on the before side (js/compare.js) [kpadopt4-ideas]
 * @property {string} compareAfter  Its label on the after side [kpadopt4-ideas]
 * @property {(before: string, after: string) => string} compareSlider  The divider's name for a screen reader [kpadopt4-ideas]
 * @property {(percent: number, before: string, after: string) => string} compareValue  Where the divider stands: "40% Before, 60% After" [kpadopt4-ideas]
 * @property {string} compareFailed  A pane whose image could not be loaded [kpadopt4-ideas]
 * @property {string} compareRetry  The Retry button of a pane whose image could not be loaded [kpadopt4 r2]
 * @property {string} compareLoading  What the busy card over a compare slider says while its images load [kpadopt4 r2]
 * @property {string} calmOn  The polite announcement when calm mode is switched on (js/calm.js) [kpadopt5]
 * @property {string} calmOff  The same, when it is switched off [kpadopt5]
 * @property {string} calmSystem  The reason the calm switch is disabled while the system itself asks for less motion [kpadopt5]
 * @property {string} shortcutsEmptyTitle  The shortcut sheet's empty state, when no shortcut is registered (js/shortcuts.js) [kpadopt5]
 * @property {string} shortcutsEmptyText  Its second line [kpadopt5]
 * @property {string} shortcutsPalette  The sheet's fixed row for the command palette key [kpadopt5]
 * @property {string} shortcutsSheet  The sheet's fixed row for the key that opens the sheet [kpadopt5]
 * @property {string} shortcutsPending  What a screen reader hears while a chord waits for its second key [kpadopt5]
 * @property {(keys: string) => string} shortcutsUnknown  The short line after a chord that does nothing, "No shortcut on g x" [kpadopt5]
 * @property {(keys: string, suggestion: string) => string} shortcutsAltGr  The warning for a key that needs AltGr on an AZERTY keyboard [kpadopt5]
 * @property {string} shortcutsUnavailable  The sheet's note on a shortcut that is not available right now [kpadopt5]
 * @property {string} ledeMore  The button that opens a collapsed lede (js/lede.js) [kpadopt5]
 * @property {string} ledeLess  The button that folds it again [kpadopt5]
 * @property {string} ledeLearnMore  The link at the end of a lede that carries `data-kp-lede-link` [kpadopt5]
 * @property {string} problemRemedy  The label before the remedy of a problem, "What now" (js/problems.js) [kpadopt5]
 * @property {(n: number) => string} problemShowOthers  The button that opens the problems folded away, "Show the other 3" [kpadopt5]
 * @property {string} problemShowFewer  The button that folds them again [kpadopt5]
 * @property {string} problemCopy  The copy button on a remedy that is a command [kpadopt5]
 * @property {(n: number) => string} problemListLabel  The name of a list of problems for a screen reader, "3 problems" [kpadopt5]
 * @property {string} flashDone  The word on a button after its work went well and the page gave no word of its own (js/flash.js) [kpadopt5]
 * @property {string} flashRefused  The word on a button after the work was refused [kpadopt5]
 * @property {string} flashFailed  The word on a button after the work failed without an answer [kpadopt5]
 * @property {string} graphShowAll  The network graph's way back to every node and every kind of link (js/graph.js) [scope-143]
 * @property {string} graphShowAllTitle  Its title [scope-143]
 * @property {string} graphHint  The hint under the graph, and the second half of its picture's accessible name: what a pointer, a click and the keys do [scope-143]
 * @property {string} graphKinds  The name of the list of kinds of link over the graph [scope-143]
 * @property {string} graphLoading  A loading graph's sentence when the page gives none (`setGraphState(el, 'loading')`) [scope-143]
 * @property {string} graphUnnamed  The picture's name when the graph has no `aria-label` [scope-143]

 * @property {string} breadcrumb
 * @property {string} pagination
 * @property {string} themePicker
 * @property {string} themeSaveFailed
 * @property {string} themeSaveRefused
 * @property {string} contractDestructive
 * @property {string} contractSemantic
 * @property {string} themeGroupLight
 * @property {string} themeGroupDark
 * @property {(requested: string, applied: string) => string} themeUnknown
 * @property {(name: string, component: string) => string} rememberClash  The console warning when two elements of one component ask to be remembered under the same name
 * @property {(theme: string, href: string) => string} registerLoadFailed  The console error when a lazily loaded register did not arrive; the theme stays what it was [scope-50]
 * @property {string} diagnosticsHeading
 * @property {string} diagnosticsStylesheet
 * @property {string} diagnosticsScript
 * @property {string} diagnosticsVersion
 * @property {string} diagnosticsThemes
 * @property {string} diagnosticsVerdict
 * @property {string} diagnosticsMatch
 * @property {(stylesheet: string, script: string) => string} diagnosticsStylesheetBehind
 * @property {(stylesheet: string, script: string) => string} diagnosticsScriptBehind
 * @property {(version: string) => string} diagnosticsThemesDiffer
 * @property {string} diagnosticsNoVersion
 * @property {(names: string) => string} diagnosticsOnlyInStylesheet
 * @property {(names: string) => string} diagnosticsOnlyInScript
 * @property {string} diagnosticsUnknownVersion
 * @property {string} triInclude  A three-way filter chip that must be in, as a screen reader hears it after the chip's name (js/segmented.js, `data-kp-segmented="tri"`) [kpadopt4]
 * @property {string} triExclude  A chip that must not be in [kpadopt4]
 * @property {string} triAny  A chip that may or may not be in [kpadopt4]
 * @property {(label: string, state: string) => string} triAnnounce  What a three-way group says after a press, "Chicken: required" [kpadopt4]
 * @property {string} triHint  The line under a three-way group while nothing is chosen [kpadopt4]
 * @property {(include: string[], exclude: string[]) => string} triSummary  The same line once something is chosen [kpadopt4]
 * @property {string} triCleared  What a three-way group says after Clear all [kpadopt4]
 * @property {(label: string) => string} quantityLess  The minus button's name, "Portions, less" (js/quantity.js) [kpadopt4]
 * @property {(label: string) => string} quantityMore  The plus button's name, "Portions, more" [kpadopt4]
 * @property {(value: string, unit: string) => string} quantityValue  The value with its unit, as a screen reader hears it, "6 portions" [kpadopt4]
 * @property {(done: number, total: number) => string} checklistCount  A checklist group's count beside its heading, "4 of 9" (js/checklist.js) [kpadopt4]
 * @property {(done: number, total: number) => string} checklistSummary  The whole list's count in its bar, "4 of 9 done" [kpadopt4]
 * @property {string} checklistCheckAll  The bar's button that ticks every item [kpadopt4]
 * @property {string} checklistUncheckAll  The bar's button that takes the tick off every item [cook-1017]
 * @property {string} checklistClear  The bar's button that takes the ticked items away [kpadopt4]
 * @property {string} checklistUndo  The button that brings them back [kpadopt4]
 * @property {(n: number) => string} checklistCleared  The bar's line after a clear, "3 cleared" [kpadopt4]
 * @property {(n: number) => string} checklistRestored  Said after Undo [kpadopt4]
 * @property {string} checklistEmpty  An empty checklist [kpadopt4]
 * @property {string} checklistLoading  The words under a loading checklist's spinner [kpadopt4]
 * @property {string} checklistFailed  A checklist that could not be read, when the page gives no message [kpadopt4]
 * @property {(label: string) => string} checklistDone  Said when an item is ticked [kpadopt4]
 * @property {(label: string) => string} checklistDoneMoved  Said when an item is ticked with the keyboard and sank [kpadopt4]
 * @property {(label: string) => string} checklistOpen  Said when an item is unticked [kpadopt4]
 * @property {string} timerRunning  A kitchen timer's state word while it runs (js/timer.js) [kpadopt4]
 * @property {string} timerDoneWord  Its state word once it has ended [kpadopt4]
 * @property {(name: string) => string} timerDone  The large line on an ended timer, its notification and its alert, "Rice is done" [kpadopt4]
 * @property {(name: string) => string} timerSpoken  What a timer says aloud when it ends and the list speaks, "Rice is done" [cook-1017]
 * @property {string} timerStopRing  The button on an ended timer that stops it and takes it away [kpadopt4]
 * @property {(name: string) => string} timerAddMinuteTo  That button's accessible name [kpadopt4]
 * @property {(name: string) => string} timerRemove  The remove button's accessible name and tooltip [kpadopt4]
 * @property {string} timerEmpty  A list with no timer [kpadopt4]
 * @property {string} timersLoading  The busy card over a timer list that is being read [r6r8-1019]
 * @property {string} timersFailed  A timer list that could not be read, when the page gives no message [r6r8-1019]
 * @property {(name: string) => string} timerLastMinute  Said once when a timer has a minute left [kpadopt4]
 * @property {(name: string, clock: string) => string} timerStartedNamed  Said when a kitchen timer starts, with its name and its clock [kpadopt4]
 * @property {(name: string) => string} timerAlready  Said when a starter is pressed while its timer runs [kpadopt4]
 * @property {(n: number, total: number) => string} focusCount  The count over a step shown on its own, "Step 3 of 8" (js/focus-steps.js) [kpadopt4]
 * @property {string} focusNext  The button that goes on to the next step [kpadopt4]
 * @property {string} focusPrevious  The button that goes back a step [kpadopt4]
 * @property {string} focusDone  The next button on the last step [kpadopt4]
 * @property {string} focusNeeds  The heading over what a step needs [kpadopt4]
 * @property {(duration: string) => string} focusTimer  A step's timer button, "Start a timer · 10 min" [kpadopt4]
 * @property {string} focusReadAloud  The button that reads the step aloud [kpadopt4]
 * @property {string} focusStopReading  The same button while it reads [kpadopt4]
 * @property {string} focusWakeOn  The screen line while the screen is kept on [kpadopt4]
 * @property {string} focusWakeOff  The screen line when it is not [kpadopt4]
 * @property {string} focusWakeUnsupported  The screen line in a browser that cannot keep it on [kpadopt4]
 * @property {string} focusEmpty  A view with no steps [kpadopt4]
 * @property {string} focusLoading  The words under its spinner while the steps load [kpadopt4]
 * @property {string} focusFailed  Steps that could not be read, when the page gives no message [kpadopt4]
 * @property {string} wheelSpin  The button in the middle of a wheel that picks (js/wheel.js) [kpadopt4]
 * @property {string} wheelIdleKiosk  The same line on a kiosk wheel, which has no button [hover-wheel-1019]
 * @property {string} wheelSpinKiosk  The name of a kiosk wheel for a screen reader: it is the one control [hover-wheel-1019]
 * @property {string} wheelResting  Added to a resting choice's tooltip [hover-wheel-1019]
 * @property {string} wheelSpinAgain  The same button once the wheel has chosen [kpadopt4]
 * @property {string} wheelIdle  The line under a wheel that has not spun yet [kpadopt4]
 * @property {string} wheelChose  The line over the outcome, "The wheel chose" [kpadopt4]
 * @property {string} wheelSpinning  The line while it turns [kpadopt4]
 * @property {string} wheelOnlyOne  The line over the outcome when there was only one choice [kpadopt4]
 * @property {string} wheelEmpty  A wheel with no choices, when the page gives no sentence [kpadopt4]
 * @property {string} wheelLoading  The words under its spinner while the choices load [kpadopt4]
 * @property {string} wheelFailed  Choices that could not be read, when the page gives no message [kpadopt4]
 * @property {string} claimAnd  The word before the last name in a list of people who bring an item (js/claim.js) [kpadopt6]
 * @property {string} claimDisabled  Why every claim button is off when the page gives no reason (js/claim.js) [kpadopt6]
 * @property {string} claimNoViewer  Why the buttons are off while nobody is signed in (js/claim.js) [kpadopt6]
 * @property {(max: number) => string} claimFullReason  Why the button is off on an item that has its people (js/claim.js) [kpadopt6]
 * @property {string} claimTake  The button's face on an item you do not bring (js/claim.js) [kpadopt6]
 * @property {string} claimHeld  The button's face on an item you bring (a press lets go) (js/claim.js) [kpadopt6]
 * @property {string} claimNobody  The words of the gap, an item nobody brings (js/claim.js) [kpadopt6]
 * @property {(names: string) => string} claimBy  The group of people on a row, as a screen reader hears it (js/claim.js) [kpadopt6]
 * @property {string} claimNobodyLabel  The same for the gap (js/claim.js) [kpadopt6]
 * @property {(label: string) => string} claimHeldLabel  The button's name on an item you bring (js/claim.js) [kpadopt6]
 * @property {(label: string) => string} claimTakeLabel  The button's name on an item you do not bring (js/claim.js) [kpadopt6]
 * @property {string} claimAssign  The assign mode's menu button (js/claim.js) [kpadopt6]
 * @property {(label: string) => string} claimAssignOf  Its name for a screen reader (js/claim.js) [kpadopt6]
 * @property {string} claimAssignHeading  The assign menu's heading (js/claim.js) [kpadopt6]
 * @property {string} claimAssigned  The hint of a person who is on the item (js/claim.js) [kpadopt6]
 * @property {string} claimNotAssigned  The hint of a person who is not (js/claim.js) [kpadopt6]
 * @property {(done: number, total: number) => string} claimGroupCount  A group's count of items somebody brings (js/claim.js) [kpadopt6]
 * @property {string} claimAllTaken  The bar's line when every item has somebody (js/claim.js) [kpadopt6]
 * @property {(n: number) => string} claimGapsCount  The bar's line with the exact number of items nobody brings (js/claim.js) [kpadopt6]
 * @property {string} claimFilterLabel  The filter chips' name (js/claim.js) [kpadopt6]
 * @property {string} claimFilterAll  The chip that shows every item (js/claim.js) [kpadopt6]
 * @property {string} claimFilterGaps  The chip that shows only the items nobody brings (js/claim.js) [kpadopt6]
 * @property {string} claimFilterMine  The chip that shows only the items you bring (js/claim.js) [kpadopt6]
 * @property {string} claimTakeAll  The bar's button that takes all items nobody brings (js/claim.js) [kpadopt6]
 * @property {string} claimCopyGaps  The bar's button that copies the items nobody brings (js/claim.js) [kpadopt6]
 * @property {string} claimNothingOpen  Why those two buttons are off (js/claim.js) [kpadopt6]
 * @property {string} claimNoneOpenTitle  The filter plate when no item is left without somebody (js/claim.js) [kpadopt6]
 * @property {string} claimNoneOpenText  Its sentence (js/claim.js) [kpadopt6]
 * @property {string} claimNoneMineTitle  The filter plate for Mine when it is empty (js/claim.js) [kpadopt6]
 * @property {string} claimNoneMineText  Its sentence (js/claim.js) [kpadopt6]
 * @property {string} claimShowAll  The plates' button (js/claim.js) [kpadopt6]
 * @property {string} claimRetry  The Retry of a list that could not be read (js/claim.js) [kpadopt6]
 * @property {string} claimNotSaved  The line when the server refused a change and gave no reason (js/claim.js) [kpadopt6]
 * @property {(name: string, label: string) => string} claimLost  The line when somebody was faster (js/claim.js) [kpadopt6]
 * @property {(label: string, names: string) => string} claimAnnounce  What is said after a change (js/claim.js) [kpadopt6]
 * @property {(label: string) => string} claimAnnounceNobody  What is said after the last person let go (js/claim.js) [kpadopt6]
 * @property {(label: string) => string} claimReleased  The toast after you let go (js/claim.js) [kpadopt6]
 * @property {(name: string, label: string) => string} claimUnassigned  The toast after a person was taken off (js/claim.js) [kpadopt6]
 * @property {(n: number) => string} claimTookAll  The toast and the line after Take every open item (js/claim.js) [kpadopt6]
 * @property {(names: string) => string} claimCopyLine  The text Copy the open list writes (js/claim.js) [kpadopt6]
 * @property {(n: number) => string} claimCopied  What is said after the copy (js/claim.js) [kpadopt6]
 * @property {string} claimCopyFailed  What is said when the clipboard refuses (js/claim.js) [kpadopt6]
 * @property {string} claimKeyTake  The shortcut sheet's line for c (js/claim.js) [kpadopt6]
 * @property {string} claimKeyGroup  The sheet's heading for the claim keys (js/claim.js) [kpadopt6]
 * @property {string} claimKeyReason  Why c does nothing now (js/claim.js) [kpadopt6]
 * @property {string} claimKeyGaps  The sheet's line for Shift C (js/claim.js) [kpadopt6]
 * @property {(n: number) => string} availabilityDaysLong  A proposed period's length under its dates (js/availability.js) [kpadopt6]
 * @property {string} availabilityYes  The answer yes, in a cell's name, the legend and the list (js/availability.js) [kpadopt6]
 * @property {string} availabilityMaybe  The answer maybe (js/availability.js) [kpadopt6]
 * @property {string} availabilityNo  The answer no (js/availability.js) [kpadopt6]
 * @property {string} availabilityNone  A cell nobody filled (js/availability.js) [kpadopt6]
 * @property {string} availabilityNobodyAnswered  The line over a grid nobody has answered (js/availability.js) [kpadopt6]
 * @property {(yes: number, total: number) => string} availabilityCount  One column's exact count (js/availability.js) [kpadopt6]
 * @property {(n: number, total: number) => string} availabilityAnswered  The line over the grid (js/availability.js) [kpadopt6]
 * @property {(labels: string, count: string) => string} availabilityBestLine  The best column in words (js/availability.js) [kpadopt6]
 * @property {string} availabilityChoose  The button on the best column (js/availability.js) [kpadopt6]
 * @property {string} availabilityUnitLabel  The organiser's switch between periods and days (js/availability.js) [kpadopt6]
 * @property {string} availabilityByPeriod  Its first choice (js/availability.js) [kpadopt6]
 * @property {string} availabilityByDay  Its second choice (js/availability.js) [kpadopt6]
 * @property {string} availabilityFailedTitle  The failed plate's heading (js/availability.js) [kpadopt6]
 * @property {string} availabilityFailedText  Its sentence (js/availability.js) [kpadopt6]
 * @property {string} availabilityEmptyTitle  The empty plate's heading (js/availability.js) [kpadopt6]
 * @property {string} availabilityEmptyText  Its sentence (js/availability.js) [kpadopt6]
 * @property {string} availabilityRetry  The failed plate's button (js/availability.js) [kpadopt6]
 * @property {string} availabilityGroup  The grid's name when the page gives none (js/availability.js) [kpadopt6]
 * @property {string} availabilitySummaryLabel  The summary row's name (js/availability.js) [kpadopt6]
 * @property {(name: string) => string} availabilityYou  Your own row's name (js/availability.js) [kpadopt6]
 * @property {string} availabilityBestWord  The legend's word for the best column (js/availability.js) [kpadopt6]
 * @property {(name: string, column: string, answer: string) => string} availabilityCell  A cell's name for a screen reader (js/availability.js) [kpadopt6]
 * @property {(label: string) => string} availabilityNotSaved  The line when a change was refused (js/availability.js) [kpadopt6]
 * @property {(label: string) => string} availabilityEveryone  Said when a column becomes complete (js/availability.js) [kpadopt6]
 * @property {(yes: number, maybe: number, no: number) => string} availabilityYourAnswers  Said after a stroke (js/availability.js) [kpadopt6]
 * @property {(n: number) => string} availabilityStroke  The toast after a stroke (js/availability.js) [kpadopt6]
 * @property {(name: string, label: string, word: string) => string} availabilityCellSaid  Said after one cell changed (js/availability.js) [kpadopt6]
 * @property {string} availabilityClear  The menu entry that empties a cell (js/availability.js) [kpadopt6]
 * @property {string} availabilityKeyYes  The shortcut sheet's line for y (js/availability.js) [kpadopt6]
 * @property {string} availabilityKeyMaybe  The line for m (js/availability.js) [kpadopt6]
 * @property {string} availabilityKeyNo  The line for n (js/availability.js) [kpadopt6]
 * @property {string} availabilityKeyClear  The line for Backspace (js/availability.js) [kpadopt6]
 * @property {string} availabilityKeyGroup  The sheet's heading (js/availability.js) [kpadopt6]
 * @property {string} availabilityKeyReason  Why those keys do nothing now (js/availability.js) [kpadopt6]
 * @property {string} availabilityDisabled  Why every cell is inert when the page gives no reason (js/availability.js) [kpadopt6]
 * @property {string} qrLabel  The code's accessible name (js/qr.js) [kpadopt6]
 * @property {string} qrReady  Said once, politely, when the code is drawn (js/qr.js) [kpadopt6]
 * @property {string} qrTooLongTitle  The failed plate's heading (js/qr.js) [kpadopt6]
 * @property {string} qrTooLongText  Its sentence (js/qr.js) [kpadopt6]
 * @property {string} qrEmptyTitle  The empty plate's heading (js/qr.js) [kpadopt6]
 * @property {string} qrEmptyText  Its sentence (js/qr.js) [kpadopt6]
 * @property {string} qrExpired  The plate of a link whose time ran out (js/qr.js) [kpadopt6]
 * @property {string} qrExpiredText  Its sentence (js/qr.js) [kpadopt6]
 * @property {string} qrHidden  The cover of a code that waits to be shown (js/qr.js) [kpadopt6]
 * @property {string} qrShow  The cover's button (js/qr.js) [kpadopt6]
 * @property {string} qrHide  The button that covers a shown code again (js/qr.js) [kpadopt6]
 * @property {string} qrLoading  The words under the busy card while the link is made (js/qr.js) [kpadopt6]
 * @property {string} qrCopy  The Copy button (js/qr.js) [kpadopt6]
 * @property {string} qrCopied  Its result word (js/qr.js) [kpadopt6]
 * @property {string} qrCopyFailed  Its refusal word (js/qr.js) [kpadopt6]
 * @property {string} qrNoLink  Why Copy is off without a link (js/qr.js) [kpadopt6]
 * @property {string} qrCopyOff  Why Copy is off while the code is not ready (js/qr.js) [kpadopt6]
 * @property {string} qrDownloadPng  The download button for a picture (js/qr.js) [kpadopt6]
 * @property {string} qrDownloadSvg  The download button for a vector file (js/qr.js) [kpadopt6]
 * @property {(bytes: number) => string} qrDense  The warning under a long link (js/qr.js) [kpadopt6]
 * @property {string} qrRetry  The failed plate's button (js/qr.js) [kpadopt6]
 * @property {string} qrKey  The shortcut sheet's line for Shift C (js/qr.js) [kpadopt6]
 * @property {string} qrKeyGroup  The sheet's heading (js/qr.js) [kpadopt6]
 * @property {string} qrKeyReason  Why the key does nothing now (js/qr.js) [kpadopt6]
 * @property {string} timeChoose  The button that opens the list of times, and the sheet's title when the field has no name (js/timefield.js) [kpadopt6]
 * @property {string} timeRequired  The line under an empty required field (js/timefield.js) [kpadopt6]
 * @property {string} timeInvalid  The line under a text that is not a time (js/timefield.js) [kpadopt6]
 * @property {(min: string, max: string) => string} timeOutside  The line under a time outside the window (js/timefield.js) [kpadopt6]
 * @property {(value: string) => string} timeConverted  The polite line after a 12 hour time was pasted (js/timefield.js) [kpadopt6]
 * @property {(value: string) => string} timeSaid  What is said after the value changed (js/timefield.js) [kpadopt6]
 * @property {string} timePresets  The name of the group of preset chips (js/timefield.js) [kpadopt6]
 * @property {string} timePresetsFailed  The compact plate when presets failed (js/timefield.js) [kpadopt6]
 * @property {string} timeRetry  Its button (js/timefield.js) [kpadopt6]
 * @property {string} timeList  The list's name for a screen reader (js/timefield.js) [kpadopt6]
 * @property {string} timeNone  The list's entry that clears the field (js/timefield.js) [kpadopt6]
 * @property {string} timeNow  The note on the list's entry for the current time (js/timefield.js) [kpadopt6]
 * @property {(busy: string) => string} timeNear  The mark on a time close to a busy moment (js/timefield.js) [kpadopt6]
 * @property {(zone: string) => string} timeZoneNote  The quiet line that says whose clock the time is (js/timefield.js) [kpadopt6]
 * @property {(value: string) => string} timeWrappedNext  Said when stepping up went past midnight (js/timefield.js) [kpadopt6]
 * @property {(value: string) => string} timeWrappedPrevious  Said when stepping down went past midnight (js/timefield.js) [kpadopt6]
 * @property {string} consoleUnit  The pill's word for a block, singular and plural (js/console.js) [kpadopt6]
 * @property {string} consoleOutputLabel  The body's name for a screen reader (js/console.js) [kpadopt6]
 * @property {string} consoleCommand  The prompt's name when the console has none (js/console.js) [kpadopt6]
 * @property {string} consoleHostFailed  The failed plate's heading and the reason the prompt is off then (js/console.js) [kpadopt6]
 * @property {string} consoleHostFailedText  The failed plate's sentence (js/console.js) [kpadopt6]
 * @property {string} consoleOff  Why the prompt is off when the page gives no reason (js/console.js) [kpadopt6]
 * @property {string} consoleContextLoadingWhy  Why the prompt is off while the context list loads (js/console.js) [kpadopt6]
 * @property {string} consoleRun  The Run button (js/console.js) [kpadopt6]
 * @property {string} consoleRunAnyway  The Run button after a line the app wants confirmed (js/console.js) [kpadopt6]
 * @property {string} consoleHint  The line under the prompt when the page gives none (js/console.js) [kpadopt6]
 * @property {string} consoleContext  The context select's label when the page gives none (js/console.js) [kpadopt6]
 * @property {string} consoleContextLoading  The select while its options load (js/console.js) [kpadopt6]
 * @property {string} consoleSending  A block before the app answered (js/console.js) [kpadopt6]
 * @property {string} consoleQueued  A queued block (js/console.js) [kpadopt6]
 * @property {string} consoleRunning  A running block with its timer off (js/console.js) [kpadopt6]
 * @property {(time: string) => string} consoleRunningFor  A running block's status with its elapsed time (js/console.js) [kpadopt6]
 * @property {(code: number) => string} consoleExit  A finished block's exit code (js/console.js) [kpadopt6]
 * @property {string} consoleDone  The word beside exit 0 (js/console.js) [kpadopt6]
 * @property {string} consoleFailed  The word beside a non-zero exit (js/console.js) [kpadopt6]
 * @property {string} consoleRefused  A block the app refused (js/console.js) [kpadopt6]
 * @property {string} consoleCancelled  A block that was stopped (js/console.js) [kpadopt6]
 * @property {string} consoleLost  A block whose connection dropped (js/console.js) [kpadopt6]
 * @property {string} consoleCopyCommand  The block's button that copies its command (js/console.js) [kpadopt6]
 * @property {string} consoleCopyOutput  The block's button that copies its output (js/console.js) [kpadopt6]
 * @property {string} consoleAgain  The block's button that runs its command again (js/console.js) [kpadopt6]
 * @property {string} consoleRecheck  The lost block's button that runs it again to see (js/console.js) [kpadopt6]
 * @property {string} consoleCancel  The running block's button that stops it (js/console.js) [kpadopt6]
 * @property {string} consoleCollapse  The block's button that folds it (js/console.js) [kpadopt6]
 * @property {string} consoleExpand  The block's button that opens it again (js/console.js) [kpadopt6]
 * @property {string} consoleCopied  A copy button's result word (js/console.js) [kpadopt6]
 * @property {string} consoleNotCopied  A copy button's refusal word (js/console.js) [kpadopt6]
 * @property {string} consoleLostWhat  A lost block's what (js/console.js) [kpadopt6]
 * @property {string} consoleLostNow  A lost block's what now (js/console.js) [kpadopt6]
 * @property {string} consoleRefusedWhat  A refusal's what when the app gave none (js/console.js) [kpadopt6]
 * @property {string} consoleNoOutput  A finished block with nothing written (js/console.js) [kpadopt6]
 * @property {string} consoleOutput  The output block's title (js/console.js) [kpadopt6]
 * @property {string} consoleWaitingOutput  A running block that has written nothing yet (js/console.js) [kpadopt6]
 * @property {string} consoleRetry  The failed plate's button (js/console.js) [kpadopt6]
 * @property {string} consoleLoading  The words under the busy card while the console loads (js/console.js) [kpadopt6]
 * @property {(n: number) => string} consoleEarlier  The line that stands for blocks folded away (js/console.js) [kpadopt6]
 * @property {string} consoleEmptyTitle  The empty plate's heading (js/console.js) [kpadopt6]
 * @property {string} consoleEmptyText  Its sentence (js/console.js) [kpadopt6]
 * @property {(code: number) => string} consoleFinishedOk  Said when the last command succeeded (js/console.js) [kpadopt6]
 * @property {(code: number) => string} consoleFinishedFailed  Said when the last command failed (js/console.js) [kpadopt6]
 * @property {string} consoleFinishedRefused  Said when the last command was refused (js/console.js) [kpadopt6]
 * @property {(command: string) => string} consoleConfirmSaid  Said when a line needs a second press (js/console.js) [kpadopt6]
 * @property {string} consoleCleared  The toast and the line after Clear (js/console.js) [kpadopt6]
 * @property {string} consoleCompletions  The completion list's name (js/console.js) [kpadopt6]
 * @property {(n: number) => string} consoleMatches  Said after Tab found several (js/console.js) [kpadopt6]
 * @property {string} consoleKeyClear  The shortcut sheet's line for Ctrl L (js/console.js) [kpadopt6]
 * @property {string} consoleKeyGroup  The sheet's heading (js/console.js) [kpadopt6]
 * @property {string} consoleKeyFocus  The sheet's line for the focus key (js/console.js) [kpadopt6]
 * @property {string} consoleKeyFocusReason  Why the focus key does nothing now (js/console.js) [kpadopt6]
 * @property {(n: number) => string} qrShowAllChars  The button that unfolds a long link under the code (js/qr.js) [kpadopt6]
 * @property {string} qrShowLess  The button that folds it again (js/qr.js) [kpadopt6]
 * @property {string} diffLabel  The diff view's accessible name when it has none (js/diff.js) [gap-diff-view]
 * @property {(files: number) => string} diffSummary  The bar's count of files: "3 files changed" (js/diff.js) [gap-diff-view]
 * @property {string} diffExpandAll  The button that opens every file (js/diff.js) [gap-diff-view]
 * @property {string} diffCollapseAll  The button that closes every file (js/diff.js) [gap-diff-view]
 * @property {string} diffAllOpen  Why Expand all is off: every file is already open (js/diff.js) [gap-diff-view]
 * @property {string} diffAllClosed  Why Collapse all is off: every file is already closed (js/diff.js) [gap-diff-view]
 * @property {string} diffStatusAdded  The badge of a new file (js/diff.js) [gap-diff-view]
 * @property {string} diffStatusRemoved  The badge of a deleted file (js/diff.js) [gap-diff-view]
 * @property {string} diffStatusModified  The badge of a changed file (js/diff.js) [gap-diff-view]
 * @property {string} diffStatusRenamed  The badge of a moved file (js/diff.js) [gap-diff-view]
 * @property {string} diffStatusBinary  The badge of a file that is not text (js/diff.js) [gap-diff-view]
 * @property {(from: string) => string} diffRenamedFrom  A renamed file's path for a screen reader: "renamed from old/path" (js/diff.js) [gap-diff-view]
 * @property {(n: number) => string} diffAddedCount  The +N count for a screen reader: "3 lines added" (js/diff.js) [gap-diff-view]
 * @property {(n: number) => string} diffRemovedCount  The -M count for a screen reader: "1 line removed" (js/diff.js) [gap-diff-view]
 * @property {(n: number) => string} diffUnchanged  The folded context row: "12 unchanged lines" (js/diff.js) [gap-diff-view]
 * @property {(n: number) => string} diffUnchangedName  That row's accessible name: "Show 12 unchanged lines" (js/diff.js) [gap-diff-view]
 * @property {(n: number) => string} diffUnchangedHide  The same row once opened: "Hide 12 unchanged lines" (js/diff.js) [gap-diff-view]
 * @property {(n: number) => string} diffUnchangedShown  Said politely after a fold opened: "12 unchanged lines shown" (js/diff.js) [gap-diff-view]
 * @property {(n: number) => string} diffUnchangedHidden  Said politely after a fold closed: "12 unchanged lines hidden" (js/diff.js) [gap-diff-view]
 * @property {(n: number) => string} diffShowMore  The row that shows the lines past the file's limit: "Show the other 1240 lines" (js/diff.js) [gap-diff-view]
 * @property {string} diffBinaryTitle  The heading of a file that is not text (js/diff.js) [gap-diff-view]
 * @property {string} diffBinaryText  Its sentence (js/diff.js) [gap-diff-view]
 * @property {string} diffNoLines  A file with no line changes (a rename or a mode change) (js/diff.js) [gap-diff-view]
 * @property {(path: string) => string} diffChangesIn  A file's body name: "Changes in stacks/media/compose.yml" (js/diff.js) [gap-diff-view]
 * @property {string} diffCopyPath  The Copy path button (js/diff.js) [gap-diff-view]
 * @property {string} diffCopied  Its result word (js/diff.js) [gap-diff-view]
 * @property {string} diffCopyFailed  Its refusal word (js/diff.js) [gap-diff-view]
 * @property {string} diffLoading  The words under the busy card while the changes are read (js/diff.js) [gap-diff-view]
 * @property {(files: number, added: number, removed: number) => string} diffReady  Said politely when the changes are drawn (js/diff.js) [gap-diff-view]
 * @property {string} diffEmptyTitle  The empty plate's heading (js/diff.js) [gap-diff-view]
 * @property {string} diffEmptyText  Its sentence (js/diff.js) [gap-diff-view]
 * @property {(name: string) => string} diffEmptySame  Its sentence when one name was compared: "compose.yml is the same on both sides." (js/diff.js) [gap-diff-view]
 * @property {(from: string, to: string) => string} diffEmptyBetween  Its sentence when two names were compared (js/diff.js) [gap-diff-view]
 * @property {string} diffFailedTitle  The failed plate's heading (js/diff.js) [gap-diff-view]
 * @property {string} diffFailedText  Its sentence (js/diff.js) [gap-diff-view]
 * @property {string} diffRetry  The failed plate's button (js/diff.js) [gap-diff-view]
 * @property {(item: string) => string} ratingGroupName  The star row's name for a screen reader: "Rating for Lasagne" (js/rating.js) [kpadopt6]
 * @property {string} ratingGroupNameBare  The same when the row names no item [kpadopt6]
 * @property {(n: number, max: number) => string} ratingStarName  One star's name: "4 of 5" [kpadopt6]
 * @property {(n: number, max: number, label: string) => string} ratingStarNameLabel  The same with its word: "4 of 5, Good" [kpadopt6]
 * @property {(value: number, max: number) => string} ratingRated  A display row's name: "Rated 4 of 5" [kpadopt6]
 * @property {(value: number, max: number, count: number) => string} ratingRatedCount  The same with how many people rated: "Rated 4.5 of 5 by 12 people" [kpadopt6]
 * @property {string} ratingUnrated  The line, and the name, of a row nobody has rated: "Not rated yet" [kpadopt6]
 * @property {string} ratingTap  The caption of an input row with no rating: "Tap a star" [kpadopt6]
 * @property {string} ratingClear  The small button that removes the rating [kpadopt6]
 * @property {(item: string) => string} ratingClearName  That button's accessible name: "Clear the rating for Lasagne" [kpadopt6]
 * @property {(value: number, max: number) => string} ratingOutOf  The caption of a rated row without words per level: "4 of 5" [kpadopt6]
 * @property {(value: number, count: number) => string} ratingAverage  An aggregated score: "4.5 (12)" [kpadopt6]
 * @property {(item: string, value: number, max: number) => string} ratingAnnounce  What is said when the rating changes: "Lasagne: 4 of 5" [kpadopt6]
 * @property {(item: string) => string} ratingAnnounceCleared  What is said when the rating is removed: "Lasagne: rating removed" [kpadopt6]
 * @property {(item: string, value: number) => string} ratingToast  The undo toast's line: "Lasagne: 4 stars" [kpadopt6]
 * @property {(item: string) => string} ratingToastCleared  The undo toast's line after Clear [kpadopt6]
 * @property {string} ratingLoading  The row's name while the saved value is read [kpadopt6]
 * @property {string} ratingFailedRead  The words that replace the stars when the value could not be read [kpadopt6]
 * @property {string} ratingRetry  Its button [kpadopt6]
 * @property {(item: string) => string} ratingSaveFailed  The line under the row when a save was refused: "Could not save the rating for Lasagne" [kpadopt6]
 * @property {string} ratingSaving  The row's description while a save is on its way [kpadopt6]
 * @property {string} ratingConfirm  The button that saves a pending rating (commit confirm) [kpadopt6]
 * @property {string} ratingCancel  The button that drops a pending rating [kpadopt6]
 * @property {(value: number, max: number) => string} ratingPending  The line over those two buttons: "4 of 5, not saved yet" [kpadopt6]
 * @property {string} ratingReadonly  What a screen reader hears on a row that cannot be changed by this person [kpadopt6]
 * @property {string} quoteBy  Who wrote a quote when the card does not say: "You" (js/quote.js) [kpadopt6]
 * @property {(by: string, when: string) => string} quoteCaption  The line under a quote: "You, Tuesday 14/10/2026" [kpadopt6]
 * @property {(weekday: string, date: string) => string} quoteDay  A date with its weekday: "Tuesday 14/10/2026" [kpadopt6]
 * @property {(days: number) => string} quoteRelative  How long ago, in words: "3 days ago", "today" [kpadopt6]
 * @property {(date: string, relative: string) => string} quoteBoth  The date and how long ago together: "Tuesday 14/10/2026 (3 days ago)" [kpadopt6]
 * @property {string} quoteHide  The button that stops a quote from coming back [kpadopt6]
 * @property {string} quoteHideDone  The undo toast's line after a hide: "Hidden" [kpadopt6]
 * @property {string} quoteHiding  What a hide that is being saved says on its button [kpadopt6]
 * @property {string} quoteCopy  The button that copies the words [kpadopt6]
 * @property {string} quoteCopied  The same button for a moment after the copy [kpadopt6]
 * @property {string} quoteCopyFailed  What is said when the clipboard refused [kpadopt6]
 * @property {string} quoteShowAll  The button that unfolds a long quote [kpadopt6]
 * @property {string} quoteShowLess  The same button once it is open [kpadopt6]
 * @property {(n: number) => string} quoteShowAllN  A list's button that shows every quote: "Show all 7" [kpadopt6]
 * @property {string} quoteShowFewer  The same button once the list is open [kpadopt6]
 * @property {string} quoteOpen  The link to the day the words were written [kpadopt6]
 * @property {string} quoteLoading  The words under the loading card [kpadopt6]
 * @property {string} quoteEmptyTitle  The empty card's title [kpadopt6]
 * @property {string} quoteEmptyText  The empty card's sentence [kpadopt6]
 * @property {string} quoteFailedTitle  The failed card's title [kpadopt6]
 * @property {string} quoteFailedText  The failed card's sentence [kpadopt6]
 * @property {string} quoteRetry  The failed card's button [kpadopt6]
 * @property {string} quoteListName  A list of quotes' name for a screen reader [kpadopt6]
 * @property {string} quoteAnnounceHidden  What is said when a quote is hidden [kpadopt6]
 * @property {string} statusOpen  A row in the calm vocabulary that is still open (js/statusrow.js, `data-kp-status-vocab="calm"`) [kpadopt6]
 * @property {string} statusDone  A calm row that is done [kpadopt6]
 * @property {string} statusPaused  A calm row that is paused [kpadopt6]
 * @property {string} statusUnknown  A calm row nothing is known about: never shown as done [kpadopt6]
 * @property {(date: string) => string} statusSinceDate  A paused row's line: "Paused since 03/10/2026" [kpadopt6]
 * @property {(duration: string) => string} statusSinceFor  The same as a length: "Paused for 3 weeks" [kpadopt6]
 * @property {(date: string, duration: string) => string} statusSinceBoth  The same with both: "Paused since 03/10/2026 (2 weeks)" [kpadopt6]
 * @property {(days: number) => string} statusDuration  A length of days in words: "5 days", "3 weeks", "4 months" [kpadopt6]
 * @property {string} statusResume  The button that takes a paused row up again [kpadopt6]
 * @property {(label: string) => string} statusResumeLabel  That button's accessible name: "Resume Beweging" [kpadopt6]
 * @property {string} statusPause  The optional button that pauses a row [kpadopt6]
 * @property {(label: string) => string} statusPauseLabel  That button's accessible name [kpadopt6]
 * @property {(label: string) => string} statusResumed  What is said and toasted after a resume: "Beweging resumed" [kpadopt6]
 * @property {(label: string) => string} statusPausedNow  What is said and toasted after a pause: "Beweging paused" [kpadopt6]
 * @property {(n: number, days: number) => string} statusFoldLine  The line that gathers long-paused rows: "3 paused for more than 30 days" [kpadopt6]
 * @property {string} statusTodayEmptyTitle  The empty today card's title [kpadopt6]
 * @property {string} statusTodayEmptyText  Its second sentence: nothing is waiting [kpadopt6]
 * @property {string} statusTodayFailedTitle  The failed today card's title [kpadopt6]
 * @property {string} statusTodayFailedText  Its sentence: the fault is the system's [kpadopt6]
 * @property {string} statusTodayLoading  The words under the loading card [kpadopt6]
 * @property {string} statusTodayRetry  The failed card's button [kpadopt6]
 * @property {string} calmLogged  A day with something logged, in words: "logged" (js/calm-tones.js, `data-kp-tones="calm"`) [kpadopt6]
 * @property {string} calmQuiet  A day with nothing logged, in words: "nothing logged", never "missed" [kpadopt6]
 * @property {string} calmPaused  A day inside a pause: "paused" [kpadopt6]
 * @property {string} calmLegendLogged  The legend's word for a logged day [kpadopt6]
 * @property {string} calmLegendQuiet  The legend's word for a quiet day [kpadopt6]
 * @property {string} calmLegendPaused  The legend's word for a paused day [kpadopt6]
 * @property {string} calmEmptyTitle  The plate over a month with nothing logged: "Nothing logged this month" [kpadopt6]
 * @property {string} calmEmptyText  Its second sentence [kpadopt6]
 * @property {string} calmFailedTitle  The plate over a calendar that could not be read [kpadopt6]
 * @property {string} calmFailedText  Its sentence: the fault is the system's [kpadopt6]
 * @property {string} calmRetry  Its button [kpadopt6]
 * @property {string} calmTrendCaption  The words of the optional caption slot under the grid: trends over the days that were logged [kpadopt6]
 * @property {(logged: number, total: number) => string} calmSummary  A calm uptime strip's summary: "21 of 30 days logged", with no incident count and no share down [kpadopt6]
 * @property {(n: number, total: number) => string} focusQuestionCount  The count over a question: "Question 3 of 7" (js/focus-steps.js, answer mode) [kpadopt6]
 * @property {(label: string, n: number, total: number) => string} focusAnswered  What a screen reader hears after an answer: "Answered Matig. Question 4 of 7" [kpadopt6]
 * @property {(label: string) => string} focusSameYesterday  The button that repeats yesterday's answer: "Same as yesterday: Matig" [kpadopt6]
 * @property {(label: string) => string} focusSameUsual  The button that takes the usual answer: "Same as usual: Goed" [kpadopt6]
 * @property {string} focusSkip  The button that passes a question: "Not today" [kpadopt6]
 * @property {string} focusSkipped  What a skipped question reads as in the summary [kpadopt6]
 * @property {string} focusReview  The next button on the last question when a summary follows [kpadopt6]
 * @property {string} focusSummaryTitle  The summary screen's name [kpadopt6]
 * @property {string} focusMultiHint  The line under a question that takes several answers [kpadopt6]
 * @property {string} focusTextLabel  The name of the box of a written answer [kpadopt6]
 * @property {string} focusPickedMark  The word a chosen answer carries for a screen reader besides the mark [kpadopt6]
 * @property {string} focusResumeAsk  The question when a check-in was left half way: "Continue where you stopped?" [kpadopt6]
 * @property {string} focusResumeContinue  Its yes [kpadopt6]
 * @property {string} focusResumeRestart  Its start over [kpadopt6]
 * @property {(n: number) => string} focusResumed  What is said when answers are put back: "Back at question 4" [kpadopt6]
 * @property {string} focusAnswerEmptyTitle  The plate of a day with no questions [kpadopt6]
 * @property {string} focusAnswerEmptyText  Its sentence: nothing is waiting [kpadopt6]
 * @property {string} focusAnswerFailedTitle  The plate of questions that could not be read [kpadopt6]
 * @property {string} focusAnswerFailedText  Its sentence [kpadopt6]
 * @property {string} focusRetry  Its button [kpadopt6]
 * @property {string} focusSaveFailedTitle  The plate when the finished answers could not be saved [kpadopt6]
 * @property {string} focusSaveFailedText  Its sentence: nothing is lost [kpadopt6]
 * @property {string} focusSaveRetry  Its button: "Try again" [kpadopt6]
 * @property {string} focusAnswerLoading  The words under the loading card of the questions [kpadopt6]
 * @property {string} focusReadEmptyTitle  The plate of a recipe with no steps (js/focus-steps.js, read mode) [focus-tracks-1019]
 * @property {string} focusReadFailedTitle  The plate of steps that could not be loaded [focus-tracks-1019]
 * @property {string} focusReadFailedText  Its sentence when the page gave no message [focus-tracks-1019]
 * @property {string} focusTrackShared  The name of the lane of steps that belong to no track: "Together" [focus-tracks-1019]
 * @property {string} focusTracksTitle  The heading over the lanes: "Tracks" [focus-tracks-1019]
 * @property {string} focusTracksHint  The line under the lanes saying how to switch [focus-tracks-1019]
 * @property {(n: number, total: number) => string} focusTracksDone  The lanes' total: "5 of 9 steps done" [focus-tracks-1019]
 * @property {(track: string, n: number, total: number) => string} focusTrackCount  The count over a step on a track: "Pasta: step 2 of 4" [focus-tracks-1019]
 * @property {(track: string, n: number) => string} focusTrackStepName  A step with no title, by place: "Pasta, step 2" [focus-tracks-1019]
 * @property {(n: number, total: number) => string} focusLaneStep  A lane's place on its track: "Step 2 of 4" [focus-tracks-1019]
 * @property {string} focusLaneHere  A lane's state word for the track you are on [focus-tracks-1019]
 * @property {string} focusLaneReady  A lane's state word for a track that can go on [focus-tracks-1019]
 * @property {(timer: string) => string} focusLaneRunning  A lane's state word while its timer runs: "Water running" [focus-tracks-1019]
 * @property {(timer: string) => string} focusLanePaused  A lane's state word while its timer is paused [focus-tracks-1019]
 * @property {(names: string) => string} focusLaneWaiting  A lane's state word while it waits: "Waiting for Sauce" [focus-tracks-1019]
 * @property {string} focusLaneRinging  A lane's state word when its timer has ended: "Time is up" [focus-tracks-1019]
 * @property {string} focusLaneFinished  A lane's state word when its track is done [focus-tracks-1019]
 * @property {string} focusTrackBand  The name of the band over the step that lists the tracks asking for you [focus-tracks-1019]
 * @property {(timer: string, track: string) => string} focusTrackTimerDone  The band's line when a step timer ended: "Water is done (Pasta)" [focus-tracks-1019]
 * @property {(track: string) => string} focusTrackReadyTitle  The band's line when a waiting track can go on: "Pasta can go on" [focus-tracks-1019]
 * @property {(step: string) => string} focusTrackNextUp  Its sentence: "Next: Add the spaghetti" [focus-tracks-1019]
 * @property {(track: string) => string} focusTrackNothingLeft  Its sentence when the track has no step left [focus-tracks-1019]
 * @property {(track: string) => string} focusTrackGo  The button that goes to a track: "Go to Pasta" [focus-tracks-1019]
 * @property {string} focusTrackGotIt  The band's button for the track you are on [focus-tracks-1019]
 * @property {(what: string) => string} focusTrackWaitTitle  The title of a step that waits: "Waiting for Boil the water" [focus-tracks-1019]
 * @property {(clock: string) => string} focusTrackWaitLeft  What a step waits for, with the time left: "07:12 left" [focus-tracks-1019]
 * @property {string} focusTrackWaitOpen  What a step waits for, with no clock: "not done yet" [focus-tracks-1019]
 * @property {(track: string) => string} focusTrackFinishedTitle  The plate of a track with no step left: "Pasta is done" [focus-tracks-1019]
 * @property {string} focusTrackFinishedText  Its sentence [focus-tracks-1019]
 * @property {(next: string) => string} focusTrackFinishedNext  Its sentence when another track can go on: "Sauce is ready for you" [focus-tracks-1019]
 * @property {(track: string) => string} focusTrackFinish  The next button on the last step of a track: "Finish Pasta" [focus-tracks-1019]
 * @property {(track: string, place: string, text: string) => string} focusTrackSwitched  What a screen reader hears after a switch of track [focus-tracks-1019]
 * @property {(step: string) => string} focusTrackUnlocked  What a screen reader hears when the step you are on stops waiting [focus-tracks-1019]
 * @property {(clock: string) => string} focusTimerRunning  A step's timer button while its timer runs: "Timer running · 07:12" [focus-tracks-1019]
 * @property {(clock: string) => string} focusTimerPaused  The same while it is paused [focus-tracks-1019]
 * @property {string} focusTimerDone  The same when it ended [focus-tracks-1019]
 * @property {string} focusTrackKeyNext  The shortcut sheet's line for T [focus-tracks-1019]
 * @property {string} focusTrackKeyPrevious  The shortcut sheet's line for Shift T [focus-tracks-1019]
 * @property {string} usualLabel  The label over the usual band of a picker (js/usual-picker.js) [kpadopt6]
 * @property {string} usualAll  The label over the full list under the band [kpadopt6]
 * @property {(usual: number, more: number) => string} usualStatus  The status line of a picker with a band: "5 usual, 33 more" [kpadopt6]
 * @property {(n: number) => string} usualShowAll  The row that unfolds the full list: "Show all 38", the exact number [kpadopt6]
 * @property {string} usualShowFewer  The same row once the list is open [kpadopt6]
 * @property {(label: string) => string} usualAdd  The row that adds what was typed: "Add 'hoofdpjn' as new" [kpadopt6]
 * @property {(name: string) => string} usualDidYouMean  A close name offered for what was typed: "Did you mean Hoofdpijn?" [kpadopt6]
 * @property {string} usualNoMatchTitle  The plate over a search that matches nothing [kpadopt6]
 * @property {string} usualNoMatchText  Its sentence when a name can be added [kpadopt6]
 * @property {string} usualNoMatchTextFixed  Its sentence when nothing can be added [kpadopt6]
 * @property {(label: string) => string} usualSelectedExisting  What is said when typed text named an item already in the list: "Hoofdpijn was already in the list, selected" [kpadopt6]
 * @property {(label: string) => string} usualAdded  What is said after an add: "Hoofdpijn added" [kpadopt6]
 * @property {(label: string) => string} usualAddedToast  The undo toast's line after an add: "Added 'Neksteun'" [kpadopt6]
 * @property {string} usualEmptyTitle  The plate of a picker with no items at all [kpadopt6]
 * @property {string} usualEmptyText  Its sentence when a name can be added [kpadopt6]
 * @property {string} usualEmptyTextFixed  Its sentence when nobody can add here [kpadopt6]
 * @property {string} usualAddButton  Its button [kpadopt6]
 * @property {string} usualFailedTitle  The plate of a list that could not be read [kpadopt6]
 * @property {string} usualFailedText  Its sentence [kpadopt6]
 * @property {string} usualRetry  Its button [kpadopt6]
 * @property {string} usualLoading  The words under the loading card of a picker [kpadopt6]
 * @property {string} usualNotToday  The check's button that says a usual choice did not happen today [kpadopt6]
 * @property {(label: string) => string} usualNotTodayName  Kept for consumers: a not-today button's accessible name, "Hoofdpijn: not today" [kpadopt6]
 * @property {(label: string) => string} usualNotTodayAnnounce  What is said when it is set: "Hoofdpijn: not today" [kpadopt6]
 * @property {string} usualNotTodayMark  The word a screen reader hears after a chip that is not today [kpadopt6]
 * @property {string} usualJump  The `?` sheet's words for Alt U: "Jump to the usual choices" [kpadopt6]
 * @property {(label: string) => string} usualAlreadyPicked  The list's line, and what is said, when a name typed is already picked: "Headache is already picked" [kpadopt6]
 * @property {(label: string) => string} usualPicked  What is said when the check's "Had it" picks a choice: "Headache: had it" [kpadopt6]
 * @property {(typed: string, label: string) => string} usualTookClosest  What is said when Enter took the closest name: "Fatigu: took Fatigue" [kpadopt6]
 * @property {(label: string) => string} usualAsk  What is said when Enter lit the first "Did you mean" row: "Did you mean Fatigue? Enter picks it" [kpadopt6]
 * @property {(label: string) => string} usualAddedNote  The line under the field after an add: "Added 'Neksteun' as new" [kpadopt6]
 * @property {string} usualUndo  Its button [kpadopt6]
 * @property {(label: string) => string} usualUndoName  That button's accessible name: "Undo adding Neksteun" [kpadopt6]
 * @property {(label: string) => string} usualAddUndone  What is said after that Undo: "Neksteun taken out again" [kpadopt6]
 * @property {string} usualCheckTitle  The heading over the check of the usual choices [kpadopt6]
 * @property {string} usualCheckWhat  The line under it in the rows look: what Not today means [kpadopt6]
 * @property {string} usualCheckWhatChips  The line under it in the chips look: what a press does and what Not today means [kpadopt6]
 * @property {string} usualHadIt  The check's button that says the choice happened today (it picks it) [kpadopt6]
 * @property {string} usualHadItWord  The word a screen reader hears after a chip that is had it [kpadopt6]
 * @property {(label: string) => string} usualUnanswered  What is said when an answer is taken back: "Headache: not answered" [kpadopt6]
 * @property {string} usualUnansweredWord  The word a screen reader hears after a chip with no answer [kpadopt6]
 * @property {string} logviewerUnit  The pill's word for a log line, singular and plural (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerLabel  The viewer's name for a screen reader when the page gives none (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerLogLabel  The scrolling list's name for a screen reader (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerSearch  The search box's label (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerSearchHint  The search box's placeholder (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerSearchNone  The count when nothing matches (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerSearchIdle  The count while the box is empty (js/logviewer.js) [gap-log-viewer]
 * @property {(n: number) => string} logviewerMatches  The count when there are matches and none is current (js/logviewer.js) [gap-log-viewer]
 * @property {(at: number, n: number) => string} logviewerMatchAt  The count with the current match (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerPrevious  The button that goes to the match before (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerNext  The button that goes to the next match (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerOnly  The toggle that hides lines without a match (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerWrap  The toggle that wraps long lines (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerTimeLabel  The time column control's name (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerTimeShort  Its first choice, the time of day (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerTimeFull  Its second choice, date and time (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerTimeOff  Its third choice, no time column (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerLevel  The level select's label (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerLevelAll  Its first entry, every level (js/logviewer.js) [gap-log-viewer]
 * @property {(level: 'info' | 'warning' | 'error') => string} logviewerLevelFrom  Its other entries: 'info', 'warning' or 'error' and above (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerCopy  The copy button (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerCopied  The copy button's result word (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerNotCopied  The copy button's refusal word (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerDownload  The download button (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerNothing  Why copy and download are off when no line is shown (js/logviewer.js) [gap-log-viewer]
 * @property {(shown: number, total: number) => string} logviewerCount  The foot's count of lines shown (js/logviewer.js) [gap-log-viewer]
 * @property {(n: number, max: number) => string} logviewerDropped  The foot's note about lines pushed out by the cap (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerLoading  The words under the busy card while the lines are read (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerEmptyTitle  The empty plate's heading (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerEmptyText  Its sentence (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerNoMatchTitle  The plate's heading when a search or level hides every line (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerNoMatchText  Its sentence (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerClear  Its button that takes the search and the level away (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerFailed  The failed plate's heading (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerFailedText  Its sentence (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerRetry  Its button (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerKeySearch  The shortcut sheet's line for the search key (js/logviewer.js) [gap-log-viewer]
 * @property {string} logviewerKeyGroup  The sheet's heading (js/logviewer.js) [gap-log-viewer]
 * @property {string} feedLabel  The accessible name of the feed's list when the host has none (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedUnitOne  The word for one row when the page names none, "event" (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedUnitMany  The word for several rows, "events" (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedToday  The heading of today's rows (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedYesterday  The heading of yesterday's rows (js/feed.js) [gap-activity-feed-1019]
 * @property {(shown: number, total: number, unit: string) => string} feedShown  The count above the list when only some rows are loaded, "24 of 129 operations" (exact numbers) (js/feed.js) [gap-activity-feed-1019]
 * @property {(n: number) => string} feedFailed  A day heading's failed count, "2 failed" (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedAllOk  A day heading when nothing failed, "all ok" (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedStateRunning  A running row, as a screen reader hears it (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedStateInfo  A row that is a note, not an outcome, as a screen reader hears it (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedBy  The word before who started a row, for a screen reader, "by Claude" (js/feed.js) [gap-activity-feed-1019]
 * @property {(title: string) => string} feedMoreLabel  The accessible name of the lines that open under a row (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedOpen  The link inside an opened row that goes where its title would (js/feed.js) [gap-activity-feed-1019]
 * @property {(reason: string, since: string) => string} feedStale  The head line of a feed whose refresh failed while its rows are still good, "Not refreshed: the host did not answer. Rows from 14:02." (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedStaleAction  The button that reads a stale feed again (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedLoading  What the busy card over the feed says (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedEmptyTitle  The empty plate's title (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedEmptyText  The empty plate's sentence when the page gives no reason (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedErrorTitle  The failed plate's title (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedErrorText  The failed plate's sentence when the page gives no reason (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedRetry  The failed plate's Retry button (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedEarlier  The button that loads earlier rows (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedEarlierLoading  The same button while it works (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedEarlierError  The sentence when earlier rows could not be loaded (js/feed.js) [gap-activity-feed-1019]
 * @property {string} feedEarlierRetry  The button that tries again (js/feed.js) [gap-activity-feed-1019]
 * @property {(shown: number, total: number, date: string) => string} feedBackTo  The note under Load earlier, "Showing 24 of 129, back to 14/10/2026" (js/feed.js) [gap-activity-feed-1019]
 * @property {(shown: number, date: string) => string} feedEnd  The foot when there is nothing earlier, "Nothing earlier: all 24 are shown, back to 14/10/2026" (js/feed.js) [gap-activity-feed-1019]
 * @property {(title: string) => string} feedNew  Announced when one new row arrived (js/feed.js) [gap-activity-feed-1019]
 * @property {(n: number, title: string) => string} feedNewMany  Announced when several arrived, naming the newest (js/feed.js) [gap-activity-feed-1019]
 */
/**
 * The defaults. English, by Kenny's decision of 2026-09-04 — the package
 * has to speak something, and English is the language a consumer is least
 * likely to have to replace.
 *
 * @type {Strings}
 */
export declare const DEFAULT_STRINGS: Strings;
/**
 * Replace some or all of the strings, for the framework-free channel and
 * for anything that reads them outside React.
 *
 * Merged rather than replaced: a consumer that wants one word does not
 * have to restate the other seventy, and a key added in a later version
 * keeps working instead of becoming `undefined` on their page.
 *
 * @param {Partial<Strings>} next
 * @returns {Strings} the merged result
 */
export declare function setStrings(next: Partial<Strings>): Strings;
/**
 * The words the stylesheets draw themselves, as custom properties
 * [scope-143]. A keyframe cannot read this module, so the word a busy
 * picture spells reaches it as `--kp-loading-word-0` to `-7`: the eight
 * ticks of cyberpunk's decipher, from all noise to the whole word. The
 * register keeps the English as each property's fallback, so a page that
 * never calls `setStrings()` draws what it always drew.
 *
 * @param {Strings} [strings] the strings to draw from, the current ones by default
 * @returns {Record<string, string>} property name to a CSS string value
 */
export declare function stringProperties(
  strings?: Strings,
): Record<string, string>;
/**
 * Write `stringProperties()` onto an element, so every stylesheet under it
 * draws the consumer's words. `setStrings()` does this on the document
 * root; call it yourself for a subtree that speaks another language.
 *
 * @param {HTMLElement} root
 * @param {Strings} [strings]
 */
export declare function applyStringProperties(
  root: HTMLElement,
  strings?: Strings,
): void;
/** @returns {Strings} the strings as they stand */
export declare function getStrings(): Strings;
/**
 * The strings a component should use: the global ones, with anything the
 * caller passed layered on top.
 *
 * @param {Partial<Strings>} [overrides]
 * @returns {Strings}
 */
export declare function resolveStrings(overrides?: Partial<Strings>): Strings;
