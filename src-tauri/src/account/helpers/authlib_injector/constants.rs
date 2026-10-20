pub static AUTHLIB_INJECTOR_JAR_NAME: &str = "authlib-injector.jar";
pub const AHNUMC_AUTH_SERVER_URL: &str = "https://skin.ahnumc.org/api/yggdrasil";
pub const AHNUMC_HOMEPAGE_URL: &str = "https://skin.ahnumc.org";
pub const AHNUMC_YGGDRASIL_CONNECT_CONFIGURATION_URL: &str =
  "https://skin.ahnumc.org/.well-known/openid-configuration/oauth";
pub static PRESET_AUTH_SERVERS: [&str; 2] = [
  AHNUMC_AUTH_SERVER_URL,
  "https://skin.mualliance.ltd/api/yggdrasil",
];
pub static SCOPE: &str =
  "openid offline_access Yggdrasil.PlayerProfiles.Select Yggdrasil.Server.Join";

pub static CLIENT_IDS: [(&str, &str); 5] = [
  // built-in preset auth servers
  ("skin.mualliance.ltd", "27"),
  // supported MUA auth servers (ref: https://github.com/SJMC-Dev/SJMCL-client-ids)
  ("hall.mc.taru.xj.cn", "yggc_kPHIuWBoMhlkThX1"),
  ("oauth.ustb.world", "6"),
  ("skin.jsumc.fun", "2"),
  ("user.suesmc.ltd", "4"),
];
