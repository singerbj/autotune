// One-time setup for signed releases and auto-update (docs: RELEASING.md). Run it on your own
// machine with the GitHub CLI logged in (`gh auth login`) as a repository admin:
//
//   node scripts/release/setup-secrets.ts [--key <file>] [--repo owner/name]
//
// It creates the updater signing key (`tauri signer generate`, password-protected) unless you
// pass an existing one, then stores:
//   variable            UPDATE_PUBKEY                the public key, baked into every release build
//   environment secret  UPDATE_SIGNING_KEY           the secret key, on the `release` environment
//   environment secret  UPDATE_SIGNING_KEY_PASSWORD  its password
// and creates the `release-prep`, `codesign` and `release` environments, limited to the default
// branch or to release tags, with you as the required reviewer where the plan allows it.
//
// The secret key only leaves your machine for GitHub's encrypted secret store. Back it up (with
// its password): if it's lost, installed copies can never update themselves again.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline/promises";
import { config } from "./config.ts";
import { decodePublicKey, decodeSecretKey } from "./minisign.ts";

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(name);
  return i < 0 ? undefined : process.argv[i + 1];
}

const repo = arg("--repo") ?? config.repo;
const keyFile = arg("--key") ?? join(homedir(), ".release-keys", `${config.slug}.key`);

function gh(args: string[], input?: string): string {
  return execFileSync("gh", args, {
    input,
    encoding: "utf8",
    stdio: [input === undefined ? "inherit" : "pipe", "pipe", "inherit"],
  });
}

async function password(prompt: string): Promise<string> {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  const answer = await rl.question(prompt);
  rl.close();
  return answer;
}

try {
  gh(["auth", "status"]);
} catch {
  console.error("Install the GitHub CLI (https://cli.github.com) and run `gh auth login` first.");
  process.exit(1);
}

let pass: string;
if (existsSync(keyFile)) {
  console.log(`Using the existing key ${keyFile}`);
  pass = await password("Its password (empty if none): ");
} else {
  pass = await password("Password for the new signing key: ");
  if (!pass) {
    console.error("Use a password: the key file on your disk is then useless on its own.");
    process.exit(1);
  }
  mkdirSync(dirname(keyFile), { recursive: true, mode: 0o700 });
  execFileSync("npx", ["--yes", "@tauri-apps/cli@2", "signer", "generate", "--ci", "-w", keyFile, "-p", pass], {
    stdio: "inherit",
  });
}

const secretKey = readFileSync(keyFile, "utf8").trim();
const publicKey = readFileSync(`${keyFile}.pub`, "utf8").trim();
// Fails on a wrong password or a .pub that isn't this key's, before anything is stored.
if (!decodeSecretKey(secretKey, pass).publicKey.equals(decodePublicKey(publicKey).key)) {
  console.error(`${keyFile}.pub is not the public key of ${keyFile}`);
  process.exit(1);
}

const me = gh(["api", "user", "--jq", ".id"]).trim();
// [environment, deployment policy]: release-prep runs on the default branch, the others on tags.
const environments: [string, { type: "branch" | "tag"; name: string }][] = [
  [
    "release-prep",
    {
      type: "branch",
      name: gh(["repo", "view", repo, "--json", "defaultBranchRef", "--jq", ".defaultBranchRef.name"]).trim(),
    },
  ],
  ["codesign", { type: "tag", name: `${config.tagPrefix}*` }],
  ["release", { type: "tag", name: `${config.tagPrefix}*` }],
];
for (const [env, policy] of environments) {
  const body = (reviewers: boolean) =>
    JSON.stringify({
      deployment_branch_policy: { protected_branches: false, custom_branch_policies: true },
      ...(reviewers && env !== "codesign" ? { reviewers: [{ type: "User", id: Number(me) }] } : {}),
    });
  try {
    gh(["api", "-X", "PUT", `repos/${repo}/environments/${env}`, "--input", "-"], body(true));
  } catch {
    console.warn(`::warning:: couldn't add you as ${env}'s required reviewer (your plan may not allow it)`);
    gh(["api", "-X", "PUT", `repos/${repo}/environments/${env}`, "--input", "-"], body(false));
  }
  try {
    gh(
      ["api", "-X", "POST", `repos/${repo}/environments/${env}/deployment-branch-policies`, "--input", "-"],
      JSON.stringify(policy),
    );
  } catch {
    // Already there.
  }
}

gh(["variable", "set", "UPDATE_PUBKEY", "--repo", repo, "--body", publicKey]);
gh(["secret", "set", "UPDATE_SIGNING_KEY", "--repo", repo, "--env", "release"], secretKey);
gh(["secret", "set", "UPDATE_SIGNING_KEY_PASSWORD", "--repo", repo, "--env", "release"], pass);

console.log(`
Done. In ${repo}:
  variable                     UPDATE_PUBKEY
  secrets (release environment) UPDATE_SIGNING_KEY, UPDATE_SIGNING_KEY_PASSWORD
  environments                 release-prep, codesign, release

BACK UP ${keyFile} and its password (a password manager is fine). If they are lost, installed
copies of ${config.productName} can never update themselves again.`);
