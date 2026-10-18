import { prepareAlecaSource } from "./source.mjs";
import { repositoryRoot } from "../../shared/paths.mjs";

const source = process.argv[2] ?? process.env.ALECA_LAYOUT_SOURCE;
const prepared = await prepareAlecaSource(repositoryRoot, source);

process.stdout.write(`AlecaFrame ${prepared.version}: ${prepared.path}\n`);
