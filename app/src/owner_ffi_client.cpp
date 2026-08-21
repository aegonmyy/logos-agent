#include "owner_ffi_client.h"

#include <QByteArray>

namespace {
const char* kLib    = "liblogos_agent.so";
const char* kEnvKey = "LOGOS_AGENT_FFI_PATH";
} // namespace

QString OwnerFfiClient::errJson(const QString& message) const
{
    QString escaped = message;
    escaped.replace(QLatin1Char('"'), QLatin1Char('\''));
    return QStringLiteral("{\"ok\":false,\"error\":\"%1\"}").arg(escaped);
}

bool OwnerFfiClient::load()
{
    if (m_loaded) {
        return true;
    }

    QString libPath = qEnvironmentVariable(kEnvKey);
    if (libPath.isEmpty()) {
        libPath = QLatin1String(kLib);
    }

    m_lib.setFileName(libPath);
    if (!m_lib.load()) {
        m_lastErr = QStringLiteral("cannot load %1: %2").arg(libPath, m_lib.errorString());
        return false;
    }

    m_free       = reinterpret_cast<FreeFn>(m_lib.resolve("logos_agent_free_string"));
    m_channelNew = reinterpret_cast<ChannelNewFn>(m_lib.resolve("logos_agent_owner_channel_new"));
    m_poll       = reinterpret_cast<PollFn>(m_lib.resolve("logos_agent_owner_poll"));
    m_decide     = reinterpret_cast<DecideFn>(m_lib.resolve("logos_agent_owner_decide"));
    m_cfgLimit    = reinterpret_cast<CfgLimitFn>(m_lib.resolve("logos_agent_owner_configure_limit"));
    m_cfgPeriod   = reinterpret_cast<CfgPeriodFn>(m_lib.resolve("logos_agent_owner_configure_period"));
    m_channelFree = reinterpret_cast<ChannelFreeFn>(m_lib.resolve("logos_agent_owner_channel_free"));

    if (!m_free || !m_channelNew || !m_poll || !m_decide
        || !m_cfgLimit || !m_cfgPeriod || !m_channelFree) {
        m_lastErr = QStringLiteral("missing owner-channel symbols in %1").arg(m_lib.fileName());
        m_lib.unload();
        return false;
    }

    m_loaded = true;
    return true;
}

bool OwnerFfiClient::open(const QString& messagingUrl,
                          const QString& agentAccountId,
                          const QString& ownerIdentity)
{
    if (!load()) {
        return false;
    }
    close();
    const QByteArray url  = messagingUrl.toUtf8();
    const QByteArray id   = agentAccountId.toUtf8();
    const QByteArray owner = ownerIdentity.toUtf8();
    m_handle = m_channelNew(url.constData(), id.constData(), owner.constData());
    if (!m_handle) {
        m_lastErr = QStringLiteral("could not open owner channel (check the agent account id)");
        return false;
    }
    return true;
}

void OwnerFfiClient::close()
{
    if (m_handle && m_channelFree) {
        m_channelFree(m_handle);
        m_handle = nullptr;
    }
}

QString OwnerFfiClient::pollRequests()
{
    if (!m_handle) {
        return errJson(QStringLiteral("owner channel not open"));
    }
    char* result = m_poll(m_handle);
    if (!result) {
        return errJson(QStringLiteral("owner poll returned null"));
    }
    const QString text = QString::fromUtf8(result);
    m_free(result);
    return text;
}

QString OwnerFfiClient::decide(const QString& requestId, bool approve)
{
    if (!m_handle) {
        return errJson(QStringLiteral("owner channel not open"));
    }
    const QByteArray id = requestId.toUtf8();
    char* result = m_decide(m_handle, id.constData(), approve);
    if (!result) {
        return errJson(QStringLiteral("owner decide returned null"));
    }
    const QString text = QString::fromUtf8(result);
    m_free(result);
    return text;
}

QString OwnerFfiClient::configureLimit(const QString& limit)
{
    if (!m_handle) {
        return errJson(QStringLiteral("owner channel not open"));
    }
    const QByteArray value = limit.toUtf8();
    char* result = m_cfgLimit(m_handle, value.constData());
    if (!result) {
        return errJson(QStringLiteral("owner configure_limit returned null"));
    }
    const QString text = QString::fromUtf8(result);
    m_free(result);
    return text;
}

QString OwnerFfiClient::configurePeriod(const QString& limit, unsigned long long seconds)
{
    if (!m_handle) {
        return errJson(QStringLiteral("owner channel not open"));
    }
    const QByteArray value = limit.toUtf8();
    char* result = m_cfgPeriod(m_handle, value.constData(), seconds);
    if (!result) {
        return errJson(QStringLiteral("owner configure_period returned null"));
    }
    const QString text = QString::fromUtf8(result);
    m_free(result);
    return text;
}

OwnerFfiClient::~OwnerFfiClient()
{
    close();
}
