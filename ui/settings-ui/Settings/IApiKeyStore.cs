using System.Security;

namespace AutoFix.SettingsUi.Settings;

public interface IApiKeyStore : IApiKeyStatus
{
    void Save(string providerProfile, SecureString key);
    void Delete(string providerProfile);
}
