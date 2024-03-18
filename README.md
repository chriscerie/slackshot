# slackshot

Export snapshots of your Slack workspace data

## Installation

* Install [Rust](https://www.rust-lang.org/tools/install)
* Run `cargo install --branch main --git https://github.com/chriscerie/slackshot`

## Usage

* [Create Slack app](https://api.slack.com/apps) for your workspace
* Create user token scopes in `Oauth & Permissions`
    - Required scopes:
        - `admin.usergroups:read`
        - `channels:history`
        - `channels:read`
        - `groups:history`
        - `im:history`
        - `im:read`
        - `mpim:history`
        - `mpim:read`
* Install app to workspace
* Copy user OAuth token
* Run `slackshot` and follow instructions
