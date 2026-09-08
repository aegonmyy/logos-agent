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
    // The to-owner topic carries two message kinds from the agent:
    //   - "approval_request": a pending over-limit spend the owner must
    //     Approve/Deny. Accumulated into requestsJson, deduped by id, removed
    //     on decide() (see below).
    //   - "spent": the agent executed a spend (owner-approved or autonomous
    //     under-limit). Accumulated into spentJson so the UI can show the
    //     agent's live activity, including stage 4's autonomous spend which
    //     posts no approval request and would otherwise be invisible.
    // Both accumulate across polls: nwaku evicts a message after its first
    // read, so a later poll returns empty. Replacing on each poll would clear
    // a still-pending request mid-flow.
    const QJsonDocument doc = QJsonDocument::fromJson(result.toUtf8());
    if (doc.isObject() && doc.object().value(QLatin1String("ok")).toBool()) {
        const QJsonArray incoming = doc.object().value(QLatin1String("requests")).toArray();
        QJsonArray mergedReq = QJsonDocument::fromJson(requestsJson().toUtf8()).array();
        QJsonArray mergedSpent = QJsonDocument::fromJson(spentJson().toUtf8()).array();
        for (const QJsonValue& v : incoming) {
            const QJsonObject obj = v.toObject();
            const QString type = obj.value(QLatin1String("type")).toString();
            if (type == QLatin1String("spent")) {
                // Dedupe spent notifications by (amount,to) pair: a re-delivered
                // message on a cumulative backend should not double-count.
                const QString amt = obj.value(QLatin1String("amount")).toString();
                const QString to = obj.value(QLatin1String("to")).toString();
                const bool known = std::any_of(mergedSpent.cbegin(), mergedSpent.cend(),
                    [&amt, &to](const QJsonValue& m) {
                        const QJsonObject mo = m.toObject();
                        return mo.value(QLatin1String("amount")).toString() == amt
                            && mo.value(QLatin1String("to")).toString() == to;
                    });
                if (!known) {
                    mergedSpent.append(v);
                }
            } else {
                // approval_request (or any other): treat as a pending request.
                const QString id = obj.value(QLatin1String("id")).toString();
                const bool known = std::any_of(mergedReq.cbegin(), mergedReq.cend(),
                    [&id](const QJsonValue& m) {
                        return m.toObject().value(QLatin1String("id")).toString() == id;
                    });
                if (!known) {
                    mergedReq.append(v);
                }
            }
        }
        setRequestsJson(QString::fromUtf8(QJsonDocument(mergedReq).toJson(QJsonDocument::Compact)));
        setSpentJson(QString::fromUtf8(QJsonDocument(mergedSpent).toJson(QJsonDocument::Compact)));
    }
    return result;
}

QString AgentOwnerPlugin::decide(QString requestId, bool approve)
{
    if (!m_owner.isOpen()) {
        return QStringLiteral("{\"ok\":false,\"error\":\"owner channel not configured\"}");
    }
    const QString result = m_owner.decide(requestId, approve);
    // Remove the decided request from the accumulated list so the Approve/Deny
    // controls disappear immediately. (A re-poll would not return it: the
    // Waku store evicted it on the first read, and the agent consumes the
    // decision and drops the pending spend.)
    QJsonArray kept = QJsonDocument::fromJson(requestsJson().toUtf8()).array();
    QJsonArray next;
    for (const QJsonValue& v : kept) {
        if (v.toObject().value(QLatin1String("id")).toString() != requestId) {
            next.append(v);
        }
    }
    setRequestsJson(QString::fromUtf8(QJsonDocument(next).toJson(QJsonDocument::Compact)));
    return result;
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
