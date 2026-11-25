# Admins invite users through an identity directory, Cognito first

Spec 0012.

Until now nobody reached an app built from the template without an account at the identity provider, made there by hand. `access` could assign roles to a `sub` before its first sign-in (ADR 0041), but nobody knows a `sub` until the provider creates the user. So an admin set someone up in the provider's console, copied their `sub`, then assigned roles in the app. And Cognito's access tokens carry no `email` or `name`, so the Users tab showed only `sub`s.

**`access` invites through a port, `IdentityDirectory`, in its Application Layer.** It creates a user who must set a password at first sign-in and has the provider mail them, resends that mail, finds a user by email (with their `sub` and whether they are pending, expired, active or missing), and deletes one. Its one adapter, `CognitoDirectory` (`crates/platform/access/core/src/cognito/`), uses the AWS SDK's `AdminCreateUser` (the email as username, `email_verified=true`, `MessageAction=RESEND` to resend), `AdminGetUser`, `AdminDeleteUser` and `DescribeUserPool` (for how long a temporary password lasts). Tests use a Fake (ADR 0004); `fake-oidc` serves a stand-in for Cognito's API at `/cognito`, so the adapter, the journeys and `make e2e` exercise the real wire format.

**An Invitation is an Aggregate Root in `access`.** It holds the email, a name, the `sub` the provider answered with, the roles, who invited and when, and a status: `pending`, `accepted` once that `sub` first signs in (in the same unit of work that records the sighting), or `revoked`. Inviting calls the provider before its unit of work, so a refusal reaches the admin at once, then saves the Invitation, assigns its roles and records the invitee as a Subject with the invitation's email and name, together. Expired isn't stored: the provider says so, and listing reports it. Revoking deletes the user at the provider and unassigns the invitation's roles, refused once the invitee has set a password. A provider user with no Invitation who is still pending (a unit of work that failed after the provider created them) is adopted by the next invitation of their email; one who has set a password is refused.

**The directory is configured by the environment, not the app's config**: `COGNITO_USER_POOL_ID` (`<region>_<id>`, naming the region) and, for a stand-in, `COGNITO_ENDPOINT`, read as `access::module` builds its provider, beside the AWS SDK's own credentials and region variables. Unset, inviting is off: its operations answer `501` (`not_configured`, a new code in the error contract), and the rest of `access` works as before. So `make init` and `crates/server/src/modules.rs` are unchanged.

**The SDK keeps ADR 0005's build.** `aws-config` and `aws-sdk-cognitoidentityprovider` are taken without default features, with an `aws-smithy-http-client` on `rustls-ring`: no `aws-lc-rs`, cmake or OpenSSL. Credentials come from the environment, profile, web identity, ECS or IMDS; SSO and `credential_process` are left out.

**The user pool** is expected to set `AllowAdminCreateUserOnly`, so an invitation is the only way in; to send mail through Cognito's default sender for now, accepting its daily limit (about 50); and to use its default invitation template. The server needs `cognito-idp:AdminCreateUser`, `AdminGetUser`, `AdminDeleteUser` and `DescribeUserPool` on the pool.

## Considered options

- **Match invitees by email at first sign-in, with no provider call.** Providers differ on whether `email` is verified, and Cognito's access tokens carry none, so anyone able to set an email could claim another's invitation. Creating the user at the provider yields the `sub` itself.
- **Invite links the app mails itself.** It would need the app to send mail (SMTP or SES) and to tie a link to a sign-in, which the provider's own temporary-password mail already does.
- **Config in `AppConfig`, passed to `access::module`.** Every app's `modules.rs`, and `make init`'s copy of it, would carry it; the environment, as the SDK's own settings, keeps it to `access`.
- **A generic OIDC or SCIM directory.** No provider-neutral admin API covers creating a user who must set a password; each provider is its own adapter, behind the same port.

## Consequences

- Cognito is the only adapter. Another provider (Entra ID, Auth0, Keycloak) is another `IdentityDirectory` adapter, chosen by its own variables.
- Social and federated sign-in through Cognito (Google, SAML or OIDC SSO) don't go through `AdminCreateUser`; when SSO is built, its users reach `access` as Subjects by signing in, and the port may gain what linking needs.
- SES and a custom invitation message (a Custom Message Lambda) are the follow-up for production mail: Cognito's default sender is rate-limited.
- The provider can drift from `access`: a pending user deleted in its console lists as `missing`, which the admin revokes.
- Lambda hosting (Spec 0010) must grant the server the IAM actions above.
