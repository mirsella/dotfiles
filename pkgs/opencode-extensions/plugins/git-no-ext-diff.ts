import { Plugin } from "@opencode/plugin";
import { parse, type Command, type Word } from "unbash";

const gitOptionsWithValues = new Set([
  "-C",
  "-c",
  "--config-env",
  "--git-dir",
  "--namespace",
  "--super-prefix",
  "--work-tree",
]);

const executable = (word: Word | undefined) => word?.value.split("/").at(-1);

function gitIndex(words: Word[]) {
  let index = 0;

  while (index < words.length) {
    switch (executable(words[index])) {
      case "git":
        return index;
      case "command":
        index++;
        while (words[index]?.value === "-p") index++;
        if (["-v", "-V"].includes(words[index]?.value)) return -1;
        if (words[index]?.value === "--") index++;
        break;
      case "env":
        index++;
        while (index < words.length) {
          const value = words[index].value;
          if (value === "--") {
            index++;
            break;
          }
          if (
            [
              "-u",
              "--unset",
              "-C",
              "--chdir",
              "-S",
              "--split-string",
              "--argv0",
            ].includes(value)
          ) {
            index += 2;
          } else if (
            value.startsWith("-") ||
            /^[A-Za-z_][A-Za-z0-9_]*=/.test(value)
          ) {
            index++;
          } else {
            break;
          }
        }
        break;
      default:
        return -1;
    }
  }

  return -1;
}

function subcommandIndex(words: Word[], git: number) {
  for (let index = git + 1; index < words.length; index++) {
    const value = words[index].value;
    if (value === "--") return index + 1;
    if (gitOptionsWithValues.has(value)) index++;
    else if (!value.startsWith("-")) return index;
  }

  return -1;
}

function rewrite(command: string) {
  const ast = parse(command);
  const insertions = new Set<number>();
  const seen = new WeakSet<object>();
  let uneditableGit = false;
  let parseError = ast.errors?.[0];

  const visit = (value: unknown, editable = true): void => {
    if (!value || typeof value !== "object" || seen.has(value)) return;
    seen.add(value);

    if (Array.isArray(value)) {
      value.forEach((child) => {
        visit(child, editable);
      });
      return;
    }

    const node = value as Record<string, unknown>;
    if (node.type === "Script") {
      const script = value as typeof ast;
      parseError ??= script.errors?.[0];
      editable &&= script.source === undefined;
    }

    if (node.type === "Command") {
      const invocation = value as Command;
      const words = invocation.name
        ? [invocation.name, ...invocation.suffix]
        : [];
      const git = gitIndex(words);
      const subcommand = git < 0 ? -1 : subcommandIndex(words, git);
      if (subcommand >= 0 && ["diff", "show"].includes(words[subcommand]?.value)) {
        let hasNoExtDiff = false;
        for (let index = subcommand + 1; index < words.length; index++) {
          if (words[index].value === "--") break;
          if (words[index].value === "--no-ext-diff") {
            hasNoExtDiff = true;
            break;
          }
        }
        if (!hasNoExtDiff) {
          if (editable) insertions.add(words[subcommand].end);
          else uneditableGit = true;
        }
      }
    }

    Object.values(node).forEach((child) => {
      visit(child, editable);
    });
    // Word.parts is a lazy prototype getter, absent from Object.values.
    visit(node.parts, editable);
  };

  visit(ast);
  if (uneditableGit)
    throw new Error(
      "Cannot safely rewrite git inside an escaped backtick substitution",
    );
  if (parseError && insertions.size)
    throw new Error(
      `Cannot safely rewrite invalid shell syntax: ${parseError.message}`,
    );

  return [...insertions]
    .sort((left, right) => right - left)
    .reduce(
      (source, index) =>
        `${source.slice(0, index)} --no-ext-diff${source.slice(index)}`,
      command,
    );
}

export default Plugin.define({
  id: "local.git-no-ext-diff",
  async setup(ctx) {
    await ctx.tool.hook("execute.before", (event) => {
      if (
        event.tool !== "shell" ||
        !event.input ||
        typeof event.input !== "object"
      )
        return;
      const args = event.input as Record<string, unknown>;
      if (typeof args.command === "string")
        args.command = rewrite(args.command);
    });
  },
});
