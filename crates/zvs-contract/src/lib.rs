pub mod accounts;
pub mod ai;
pub mod browser;
pub mod channels;
pub mod enums;
pub mod envelope;
pub mod errors;
pub mod events;
pub mod ids;
pub mod primitives;
pub mod providers;
pub mod secrets;
pub mod settings;
pub mod system;
pub mod validate;
pub mod wire;

pub use accounts::{
    AccountDto, AccountLinkInput, AccountLinkResult, AccountRef, AccountsCancelLinkOutput,
    AccountsListOutput, AccountsUnlinkOutput,
};
pub use ai::{
    AccountFamily, AccountStatus, AdapterCapabilitiesDto, AdapterDescriptorDto, AdapterFamily,
    AuthMode, HonoursDto, LOCAL_BASE_URL, ProvidersAdaptersOutput, ToolMode, TunableParameter,
    model_tool_support, resolve_tool_mode,
};
pub use browser::{
    BrowserBounds, BrowserCommand, BrowserCommandOutput, BrowserCookie, BrowserSecurity,
    BrowserState, BrowserTab,
};
pub use channels::{CHANNELS, is_channel};
pub use enums::WireEnum;
pub use envelope::ResultEnvelope;
pub use errors::{AppError, AppErrorCode, ErrorDetails};
pub use events::{
    ApprovalRequestDto, DeltaKind, EVENT_CHANNEL, EventSeq, HostEvent, HostEventDraft, LogLineDto,
    RunOutcomeDto, RunOutcomeStatus, StepEventDto,
};
pub use ids::*;
pub use primitives::{Json, MAX_SAFE_INTEGER, Page, PageRequest, Sort, SortDirection, Timestamp};
pub use providers::{
    CreateProviderInput, DiscoveredModelDto, ModelCapability, ModelDto, ProbeOutcomeDto,
    ProbeRequest, ProbeResultDto, ProviderCapability, ProviderConnectionInput, ProviderDto,
    ProviderKind, ProviderListFilter, ProviderRef, ProviderSettingsDto, ProviderStatus,
    ProviderSummaryDto, ProvidersGetInput, ProvidersListOutput, ProvidersRemoveOutput,
    SetDefaultModelInput, UpdateProviderInput,
};
pub use secrets::{
    CreateSecretInput, FieldDefault, FieldDescriptor, FieldKind, FieldOption, RotationStatus,
    SecretDto, SecretFields, SecretFilter, SecretInput, SecretRef, SecretScope, SecretSummaryDto,
    SecretTypeKey, SecretTypeSchema, SecretsListOutput, SecretsRemoveOutput, SecretsTypesOutput,
    UpdateSecretInput, find_secret_type_schema, is_secret_type_key, secret_type_registry,
};
pub use settings::{
    SettingsGetInput, SettingsGetOutput, SettingsSetInput, SettingsSetOutput, check_setting_key,
};
pub use system::{
    DeviceGpuDto, DeviceProfileDto, DeviceTier, ResourceSampleDto, SystemDemoJobInput,
    SystemDemoStreamInput, SystemDemoStreamOutput, SystemDeviceInput, SystemNativePingInput,
    SystemNativePingOutput, SystemPingInput, SystemPingOutput,
};
pub use validate::{VALIDATION_MESSAGE, Validate, parse};
