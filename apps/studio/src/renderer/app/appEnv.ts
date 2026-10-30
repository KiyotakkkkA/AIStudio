declare const __ZVS_APP_ENV__: Readonly<{
  ZVS_APP_VERSION: string;
  ZVS_APP_CHANNEL: string;
  ZVS_CLI_VERSION: string;
  ZVS_CLI_CHANNEL: string;
}>;

export const APP_VERSION = __ZVS_APP_ENV__.ZVS_APP_VERSION;
export const APP_CHANNEL = __ZVS_APP_ENV__.ZVS_APP_CHANNEL;
export const CLI_VERSION = __ZVS_APP_ENV__.ZVS_CLI_VERSION;
export const CLI_CHANNEL = __ZVS_APP_ENV__.ZVS_CLI_CHANNEL;
