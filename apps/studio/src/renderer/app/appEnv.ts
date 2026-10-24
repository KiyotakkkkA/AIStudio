declare const __ZVS_APP_ENV__: Readonly<{ ZVS_APP_VERSION: string; ZVS_APP_CHANNEL: string }>;

export const APP_VERSION = __ZVS_APP_ENV__.ZVS_APP_VERSION;
export const APP_CHANNEL = __ZVS_APP_ENV__.ZVS_APP_CHANNEL;
