export type NavStatus = "accent" | "ok" | "warn" | "err";

export interface NavItemModel {
  readonly id: string;
  readonly label: string;
  readonly icon: string;
  readonly path?: string;
  readonly external?: boolean;
  readonly badge?: number;
  readonly status?: NavStatus;
}

export interface NavGroupModel {
  readonly id: string;
  readonly label: string;
  readonly items: readonly NavItemModel[];
}

export interface RailIdentity {
  readonly initials: string;
  readonly name: string;
  readonly hint: string;
}
