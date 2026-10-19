// The kp-themes module chassis-rs serves at /static/kp/js/command.js (kp-themes 11.0.0),
// copied from kp-themes js/command.d.ts: chassis serves the module, not its types.
/** A command block. */
export declare const COMMAND = "[data-kp-command]";
/** The terminals the package knows, with their prompt and the line ends that carry a command on to the next line. */
export declare const COMMAND_SHELLS: {
  readonly wsl: {
    readonly prompt: "$";
    readonly continues: readonly ["\\", "|", "&&", "||"];
    readonly windows: false;
  };
  readonly garuda: {
    readonly prompt: "$";
    readonly continues: readonly ["\\", "|", "&&", "||"];
    readonly windows: false;
  };
  readonly bash: {
    readonly prompt: "$";
    readonly continues: readonly ["\\", "|", "&&", "||"];
    readonly windows: false;
  };
  readonly powershell: {
    readonly prompt: "PS>";
    readonly continues: readonly ["`", "|"];
    readonly windows: true;
  };
  readonly cmd: {
    readonly prompt: ">";
    readonly continues: readonly ["^"];
    readonly windows: true;
  };
};
/**
 * The lines of a command, each with whether it carries on from the line before.
 * @param {string} text the command as written
 * @param {readonly string[]} continues the line ends that carry the command on (a continuation mark, a pipe)
 * @returns {{ text: string, continued: boolean }[]}
 */
export declare function commandLines(
  text: string,
  continues: readonly string[],
): {
  text: string;
  continued: boolean;
}[];
/**
 * Wire every command block under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachCommands(root?: ParentNode): () => void;
