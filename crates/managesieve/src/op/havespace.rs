/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs Ltd <hello@stalw.art>
 *
 * SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-SEL
 */

use imap_proto::receiver::Request;
use tokio::io::{AsyncRead, AsyncWrite};
use trc::AddContext;

use crate::core::{Command, ResponseCode, Session, StatusResponse};

impl<T: AsyncRead + AsyncWrite> Session<T> {
    pub async fn handle_havespace(&mut self, request: Request<Command>) -> trc::Result<Vec<u8>> {
        let mut tokens = request.tokens.into_iter();
        let name = tokens
            .next()
            .and_then(|s| s.unwrap_string().ok())
            .ok_or_else(|| {
                trc::Cause::ManageSieve
                    .into_err()
                    .details("Expected script name as a parameter.")
            })?;
        let size: usize = tokens
            .next()
            .and_then(|s| s.unwrap_string().ok())
            .ok_or_else(|| {
                trc::Cause::ManageSieve
                    .into_err()
                    .details("Expected script size as a parameter.")
            })?
            .parse::<usize>()
            .map_err(|_| {
                trc::Cause::ManageSieve
                    .into_err()
                    .details("Invalid size parameter.")
            })?;

        // Validate name
        let access_token = self.state.access_token();
        let account_id = access_token.primary_id();
        self.validate_name(account_id, &name).await?;

        // Validate quota
        if access_token.quota == 0
            || size as i64
                + self
                    .jmap
                    .get_used_quota(account_id)
                    .await
                    .caused_by(trc::location!())?
                <= access_token.quota as i64
        {
            Ok(StatusResponse::ok("").into_bytes())
        } else {
            Err(trc::Cause::ManageSieve
                .into_err()
                .details("Quota exceeded.")
                .code(ResponseCode::QuotaMaxSize))
        }
    }
}
