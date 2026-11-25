---
status: ready-for-tickets
---

# Admins invite users through the identity provider

## Problem

Nobody can get into an app built from the template unless they already have an account in the identity provider. Spec 0001 lets an admin assign roles to a `sub` before its first sign-in, but nobody knows a `sub` until the provider creates the user. So setting someone up means working in the AWS console, copying their `sub`, then assigning roles in the app. Cognito access tokens also don't carry `email` or `name`, so the Users tab shows only `sub`.

## Solution overview

The platform module `access` gains **Invitations**. An admin enters an email, a name and roles. `access` asks the identity provider to create the user, assigns the roles to the `sub` the provider returns, and the provider emails the invitee. The provider sits behind a port, `IdentityDirectory`, in `access`'s Application Layer. Cognito is the only adapter, and a Fake is used in tests and `make e2e` (ADR 0004). Without a configured directory, inviting is off and the rest of `access` works as before.

## Behaviour

### Invitation

- An Invitation is an Aggregate Root in `access`. It has: id, email (a Value Object), name, `sub`, roles, invited by (the inviter's `sub`), created at, status (`pending`, `accepted` or `revoked`), and a version.
- Inviting:
  - Every role must exist, otherwise `400`, naming the field. `user` is implicit and refused with `400`, as in spec 0001.
  - An email that already has a pending Invitation is refused with `409`.
  - An email the directory already has as an active user is refused with `409`, and the message tells the admin to assign roles instead.
- `access` calls the directory before opening its Unit of Work, so the provider's refusals reach the admin immediately: a bad email becomes `400`, an existing user `409`, and an unreachable provider `503`. One Unit of Work then saves the Invitation and its roles' Assignments, bumps the revision and publishes `InvitationCreated` and `RoleAssigned`.
- If that Unit of Work fails after the directory created the user, the next attempt for that email finds the user already exists. If the user is still waiting to set their password and no Invitation names them, `access` adopts them rather than refusing.
- The invitee's `sub` is recorded as a Subject at once, with the invitation's email and name, so the Users tab shows them before they sign in. A later sighting's details replace them only when the token carries them.
- It becomes `accepted` when its `sub` is first seen signing in, in the same Unit of Work that records the sighting, publishing `InvitationAccepted`.
- **Resend:**
  - Allowed only while the Invitation is `pending`.
  - The directory re-sends the invite email with a fresh temporary password.
  - Takes `If-Match`.
- **Revoke:**
  - Allowed only while the Invitation is `pending`.
  - Deletes the user from the directory, unassigns the Invitation's roles and publishes `InvitationRevoked` and `RoleUnassigned`.
  - Takes `If-Match`.
  - If the directory reports that the user has already set their password, revoking is refused with `409`, and the admin removes roles instead.
- **Expired** is not a stored status. Listing reports a pending Invitation as expired when the directory says its temporary password has lapsed. Resending clears it.

### Operations

Each has an `operation_id` and a summary written for an agent (ADR 0034), so each is also an MCP tool: `invite_user`, `list_invitations` (filtered by status), `resend_invitation`, `revoke_invitation`. Each is guarded like `access`'s other operations: `admin` reaches them through baseline rules. When no directory is configured, all four answer `501` (the operations still appear in the spec).

### Cognito

- The adapter uses `aws-sdk-cognitoidentityprovider`:
  - `AdminCreateUser` with `email` and `email_verified=true` as attributes, the email as username, and Cognito's default invite message.
  - `AdminCreateUser` with `MessageAction=RESEND` to resend.
  - `AdminGetUser` for status (`FORCE_CHANGE_PASSWORD` means pending, and lapsed temporary passwords are reported as expired).
  - `AdminDeleteUser` to revoke.
- Config: `COGNITO_USER_POOL_ID`. The region is taken from the pool id. Credentials come from the default AWS chain. `.env.example` documents it, along with the IAM actions the server needs: `cognito-idp:AdminCreateUser`, `AdminGetUser` and `AdminDeleteUser`.
- Docs (a new ADR) record that the user pool:
  - sets `AllowAdminCreateUserOnly`, so an invitation is the only way in;
  - sends mail through Cognito's default sender for now, accepting its daily limit;
  - uses its default invitation template.

### Fake and `make e2e`

- `crates/common/fake-oidc` gains a fake directory. It creates users the fake provider can then sign in as, and records the invite mail it would send.
- A journey test: an admin invites a user with a role, the invitee signs in through the fake provider, the guarded operation the role permits succeeds, and the Invitation is `accepted`.
- `make e2e` extends the sign-in script: invite from the Invitations tab, then see the pending row.

### Screen

- The access admin screen gains an **Invitations** tab beside Roles, Users and Grants.
- The tab lists Invitations by status, with email, name, roles, inviter, and when each was created.
- An "Invite user" dialog takes email, name and roles (the role picker from spec 0001).
- Pending rows offer resend and revoke, with a confirmation step. Refusals are shown.
- When the directory isn't configured, the tab says inviting is off.

### Seeds and `make init`

- Development seeds create no Invitations.
- After `make init`, `access` keeps these operations and the tab. Inviting stays off until `COGNITO_USER_POOL_ID` is set.

### Docs

- `docs/platform/glossary.md` gains **Invitation** and **Identity Directory**.
- A new ADR for the directory port and the Cognito choice.

## Out of scope

- Social and federated sign-in (Google, SAML or OIDC SSO through Cognito). SSO is expected later. The port should not rule it out, but nothing here builds it.
- SES and custom invite emails (a Custom Message Lambda). Planned later. The ADR names them as the follow-up.
- Identity providers other than Cognito.
- Bulk invites, invite links, and invitations that expire on the app's side.
- Disabling or deleting users who have already accepted.
- Provisioning the user pool as infrastructure code.

## Open risks

- **The provider can drift from `access`.** A user deleted in the console while still pending shows as an error when it is resent. The listing should report "not found in directory" rather than fail.
- **Cognito's default email limit** (about 50 a day) can block inviting in a busy environment before SES lands.
- **Spec 0010 (Lambda)** must give the server the IAM actions above, and network access to Cognito if it ever runs inside a VPC.
