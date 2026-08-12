// 和 Rust 端 `accounts::AccountSummary` 字段一一对应（serde 默认 snake_case）。
export interface AccountSummary {
  id: string;
  github_login: string | null;
  display_name: string | null;
  email: string | null;
  avatar_url: string | null;
  plan_raw: string | null;
  is_current: boolean;
  token_spend_used_cents: number | null;
  token_spend_limit_cents: number | null;
  token_spend_remaining_cents: number | null;
  edit_predictions_used: number | null;
  edit_predictions_limit_raw: string | null;
  edit_predictions_remaining_raw: string | null;
  billing_period_end_at: number | null;
  last_refreshed_at: number | null;
  last_quota_error: string | null;
  has_web_session: boolean;
  web_profile_id: string | null;
}

export interface PendingLoginDto {
  login_id: string;
  verification_uri: string;
  profile_id: string;
}

export type LoginPollResult =
  | { status: "pending" }
  | { status: "success"; account: AccountSummary }
  | { status: "error"; message: string };
