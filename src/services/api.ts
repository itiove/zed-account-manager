import { invoke } from "@tauri-apps/api/core";
import type { AccountSummary, LoginPollResult, PendingLoginDto } from "../types";

export interface PrepareBrowserDto {
  account_id: string;
  profile_id: string;
  initial_url: string;
}


export const api = {
  listAccounts: () => invoke<AccountSummary[]>("list_accounts"),
  loginStart: () => invoke<PendingLoginDto>("login_start"),
  loginCancel: () => invoke<void>("login_cancel"),
  loginPoll: (loginId: string) =>
    invoke<LoginPollResult>("login_poll", { loginId }),
  switchAccount: (accountId: string) =>
    invoke<AccountSummary>("switch_account", { accountId }),
  refreshQuota: (accountId?: string) =>
    invoke<AccountSummary[]>("refresh_quota", { accountId: accountId ?? null }),
  prepareAccountBrowser: (accountId: string) =>
    invoke<PrepareBrowserDto>("prepare_account_browser", { accountId }),
  recaptureWebSession: (accountId: string) =>
    invoke<AccountSummary>("recapture_web_session", { accountId }),
  logoutCurrent: () => invoke<void>("logout_current"),
  removeAccount: (accountId: string) =>
    invoke<void>("remove_account", { accountId }),

  browserOpen: (url: string, profileId: string, chromeHeight: number) =>
    invoke<void>("browser_open", { url, profileId, chromeHeight }),
  browserClose: () => invoke<void>("browser_close"),
};
