#include "agent_owner_plugin.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QTimer>

#include "logos_api.h"
#include "logos_api_client.h"

namespace {
// Owner-channel wiring: the agent this app owns, the owner identity, and the
// Logos Messaging (Waku REST) node both sides talk over. All three default
// empty, which leaves the channel closed until the deployment sets them.
const char* kAgentAccountEnv = "LOGOS_AGENT_ACCOUNT_ID";
const char* kOwnerIdentityEnv = "LOGOS_AGENT_OWNER_ID";
const char* kMessagingUrlEnv = "AGENT_MESSAGING_URL";
} // namespace

AgentOwnerPlugin::AgentOwnerPlugin(QObject* parent)
    : AgentOwnerSimpleSource(parent)
{
}

AgentOwnerPlugin::~AgentOwnerPlugin()
{
    delete m_client;
}

void AgentOwnerPlugin::initLogos(LogosAPI* api)
{
    m_api = api;
    setBackend(this);
    ensureClient();
    ensureOwnerChannel();
    QTimer::singleShot(0, this, [this]() { refresh(); });
}

void AgentOwnerPlugin::ensureClient()
{
    if (m_client || !m_api) {
        return;
    }
    m_client = new LogosAPIClient(
        QStringLiteral("agent"),
        QStringLiteral("agent_owner"),
        m_api->getTokenManager(),
        this);
}

void AgentOwnerPlugin::ensureOwnerChannel()
{
    if (m_owner.isOpen()) {
        return;
    }
    const QString agentAccount = qEnvironmentVariable(kAgentAccountEnv);
    const QString ownerIdentity = qEnvironmentVariable(kOwnerIdentityEnv);
    const QString messagingUrl = qEnvironmentVariable(kMessagingUrlEnv);
    if (agentAccount.isEmpty() || ownerIdentity.isEmpty()) {
        // Not configured: the channel stays closed and the UI says so, rather
        // than silently showing an empty approvals list.
        setChannelOpen(false);
        setRequestsJson(QStringLiteral("[]"));
        return;
    }
    if (!m_owner.open(messagingUrl, agentAccount, ownerIdentity)) {
        setLastErr(m_owner.lastError());
        setChannelOpen(false);
        return;
    }
    setChannelOpen(true);
}

QString AgentOwnerPlugin::invokeAgent(const QString& method, const QVariantList& args)
{
    ensureClient();
    if (!m_client) {
        return QStringLiteral("{\"ok\":false,\"error\":\"no agent client\"}");
    }
    switch (args.size()) {
        case 0:
            return m_client->invokeRemoteMethod(QStringLiteral("agent"), method).toString();
        case 1:
            return m_client->invokeRemoteMethod(QStringLiteral("agent"), method, args[0]).toString();
        case 2:
            return m_client->invokeRemoteMethod(QStringLiteral("agent"), method, args[0], args[1]).toString();
        default:
            return QStringLiteral("{\"ok\":false,\"error\":\"unsupported argument count\"}");
    }
}

void AgentOwnerPlugin::refresh()
{
    setAgentVersion(invokeAgent(QStringLiteral("agentVersionJson")));
    setSkillsJson(invokeAgent(QStringLiteral("skillsJson")));
}

QString AgentOwnerPlugin::invokeSkill(QString name, QString argsJson)
{
    return invokeAgent(QStringLiteral("invokeSkillJson"), QVariantList{ name, argsJson });
}

QString AgentOwnerPlugin::pollRequests()
{
    ensureOwnerChannel();
    if (!m_owner.isOpen()) {
        setRequestsJson(QStringLiteral("[]"));
        return QStringLiteral("{\"ok\":false,\"error\":\"owner channel not configured\"}");
    }
    const QString result = m_owner.pollRequests();
    // Surface the requests array for the QML list even when the envelope
    // reports an error, so the UI can show the error string.
    const QJsonDocument doc = QJsonDocument::fromJson(result.toUtf8());
    if (doc.isObject() && doc.object().value(QLatin1String("ok")).toBool()) {
        const QJsonArray requests = doc.object().value(QLatin1String("requests")).toArray();
        setRequestsJson(QString::fromUtf8(QJsonDocument(requests).toJson(QJsonDocument::Compact)));
    }
    return result;
}

QString AgentOwnerPlugin::decide(QString requestId, bool approve)
{
    if (!m_owner.isOpen()) {
        return QStringLiteral("{\"ok\":false,\"error\":\"owner channel not configured\"}");
    }
    return m_owner.decide(requestId, approve);
}

QString AgentOwnerPlugin::configureLimit(QString limit)
{
    if (!m_owner.isOpen()) {
        return QStringLiteral("{\"ok\":false,\"error\":\"owner channel not configured\"}");
    }
    return m_owner.configureLimit(limit);
}

QString AgentOwnerPlugin::configurePeriod(QString limit, qulonglong seconds)
{
    if (!m_owner.isOpen()) {
        return QStringLiteral("{\"ok\":false,\"error\":\"owner channel not configured\"}");
    }
    return m_owner.configurePeriod(limit, seconds);
}
