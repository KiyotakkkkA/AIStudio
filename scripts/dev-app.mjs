export const DEV_APP_VAR = "ZVS_DEV_APP";
export const DEV_APPS = ["studio", "boot", "browser", "computer", "recovery"];

export function takeDevApp(argv) {
  const rest = [];
  let app;
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--app") {
      app = argv[index + 1];
      index += 1;
    } else if (argument.startsWith("--app=")) {
      app = argument.slice("--app=".length);
    } else {
      rest.push(argument);
    }
  }
  if (app !== undefined && !DEV_APPS.includes(app)) {
    throw new Error(`--app expects one of: ${DEV_APPS.join(", ")}; got "${app ?? ""}"`);
  }
  return { app, rest };
}
