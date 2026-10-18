import { fileURLToPath } from "node:url";

export const toolDirectory = fileURLToPath(new URL("../", import.meta.url));
export const repositoryRoot = fileURLToPath(new URL("../../../", import.meta.url));
