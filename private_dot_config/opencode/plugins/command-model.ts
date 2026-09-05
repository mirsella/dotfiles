import type { Plugin } from "@opencode-ai/plugin";

type CommandModel = {
  providerID: string;
  modelID: string;
  variant: string | undefined;
};

export default (async () => {
  const commandModels = new Map<string, CommandModel>();
  const pending = new Map<string, CommandModel>();

  return {
    config: async (config) => {
      for (const [name, command] of Object.entries(config.command ?? {})) {
        if (command.model === undefined || command.subtask === true) continue;
        const match = /^([^/\s#]+)\/([^\s#]+)(?:#([^\s#]+))?$/.exec(command.model);
        if (!match) throw new Error(`Invalid model for /${name}: expected provider/model[#variant]`);
        const [, providerID, modelID, variant] = match;
        commandModels.set(name, { providerID, modelID, variant });
        // Prevent the built-in command override from changing session selection.
        delete command.model;
      }
    },
    "command.execute.before": async ({ command, sessionID }) => {
      const model = commandModels.get(command);
      if (model) pending.set(sessionID, model);
      else pending.delete(sessionID);
    },
    "chat.message": async ({ sessionID }, { message }) => {
      const model = pending.get(sessionID);
      pending.delete(sessionID);
      // V1 reads variant here, though the installed legacy SDK type omits it.
      if (model) message.model = { ...model };
    },
    event: async ({ event }) => {
      if (event.type === "session.deleted") pending.delete(event.properties.info.id);
    },
  };
}) satisfies Plugin;
