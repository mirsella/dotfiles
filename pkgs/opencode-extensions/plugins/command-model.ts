import type { Plugin } from "@opencode-ai/plugin";
import { parseModel, type ModelSpec } from "../lib/model-spec";

export default (async () => {
  const commandModels = new Map<string, ModelSpec>();
  const pending = new Map<string, ModelSpec>();

  return {
    config: async (config) => {
      for (const [name, command] of Object.entries(config.command ?? {})) {
        if (
          command.model === undefined ||
          command.subtask === true ||
          (command as { subagent?: boolean }).subagent === true
        )
          continue;
        const model = parseModel(command.model);
        if (model === undefined) {
          throw new Error(`Invalid model for /${name}: expected provider/model[#variant]`);
        }
        commandModels.set(name, model);
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
      // Never switch the session to a provider other than the one it is already using.
      if (!model || message.model.providerID !== model.providerID) return;
      const fast = model.providerID === "openai"
        && message.model.modelID.endsWith("-fast")
        && !model.modelID.endsWith("-fast");
      // V1 reads variant here, though the installed legacy SDK type omits it.
      message.model = { ...model, modelID: fast ? `${model.modelID}-fast` : model.modelID };
    },
    event: async ({ event }) => {
      if (event.type === "session.deleted") pending.delete(event.properties.info.id);
    },
  };
}) satisfies Plugin;
