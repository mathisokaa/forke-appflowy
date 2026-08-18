use app_error::ErrorCode;
use client_api_test::*;

#[tokio::test]
async fn sign_in_unknown_user() {
  let email = generate_unique_email();
  let password = "Hello123!";
  let c = localhost_client();
  let err = c.sign_in_password(&email, password).await.unwrap_err();
  assert_eq!(err.code, ErrorCode::OAuthError, "{:?}", err);
  assert!(!err.message.is_empty());
}

#[tokio::test]
async fn sign_in_wrong_password() {
  let c = localhost_client();

  let email = generate_unique_email();
  let password = "Hello123!";
  c.sign_up(&email, password).await.unwrap();

  let wrong_password = "Hllo123!";
  let err = c
    .sign_in_password(&email, wrong_password)
    .await
    .unwrap_err();
  assert_eq!(err.code, ErrorCode::OAuthError, "{:?}", err);
  assert!(!err.message.is_empty());
}

#[tokio::test]
async fn sign_in_unconfirmed_email() {
  let c = localhost_client();

  let email = generate_unique_email();
  let password = "Hello123!";
  c.sign_up(&email, password).await.unwrap();

  let err = c.sign_in_password(&email, password).await.unwrap_err();
  assert_eq!(err.code, ErrorCode::OAuthError, "{:?}", err);
  assert!(!err.message.is_empty());
}

#[tokio::test]
async fn sign_in_success() {
  let registered_user = generate_unique_registered_user().await;

  {
    // First Time
    let c = localhost_client();
    let is_new = c
      .sign_in_password(&registered_user.email, &registered_user.password)
      .await
      .unwrap()
      .is_new;
    assert!(is_new);
    assert!(c
      .token()
      .read()
      .as_ref()
      .unwrap()
      .user
      .confirmed_at
      .is_some());

    let workspaces = c.get_workspaces().await.unwrap();
    assert_eq!(workspaces.len(), 1);
    let _ = c.get_profile().await.unwrap();
  }

  {
    // Subsequent Times
    let c = localhost_client();
    let is_new = c
      .sign_in_password(&registered_user.email, &registered_user.password)
      .await
      .unwrap()
      .is_new;
    assert!(!is_new);

    // workspaces should be the same
    let workspaces = c.get_workspaces().await.unwrap();
    assert_eq!(workspaces.len(), 1);
  }
}

#[tokio::test]
async fn sign_in_with_invalid_url() {
  // NOTE: upstream fixture originally embedded a fake/expired OAuth JWT + provider token here;
  // redacted in this vendored copy to satisfy GitHub secret-scanning push protection.
  let url_str = "appflowy-flutter://#access_token=REDACTED_TEST_FIXTURE_JWT&expires_in=3600&provider_token=REDACTED_TEST_FIXTURE_PROVIDER_TOKEN&refresh_token=REDACTED_TEST_FIXTURE_REFRESH_TOKEN&token_type=bearer";
  let c = localhost_client();
  match c.sign_in_with_url(url_str).await {
    Ok(_) => panic!("should not be ok"),
    Err(e) => assert_eq!(e.code, ErrorCode::OAuthError, "{:?}", e),
  }
}

#[tokio::test]
async fn sign_in_with_url() {
  let c = localhost_client();
  let email = generate_unique_email();
  let action_link = generate_sign_in_action_link(&email).await;
  let sign_in_url = c.extract_sign_in_url(action_link.as_str()).await.unwrap();
  let is_new = c.sign_in_with_url(&sign_in_url).await.unwrap();
  assert!(is_new);
}

#[tokio::test]
async fn sign_in_with_magic_link() {
  let c = localhost_client();
  let email = generate_unique_email();
  let resp = c.sign_in_with_magic_link(&email, None).await;
  assert!(resp.is_ok());
}
