#ifndef OWNER_FFI_CLIENT_H
#define OWNER_FFI_CLIENT_H

#include <QLibrary>
#include <QString>

// Lazy-loads liblogos_agent.so and exposes the owner-channel C entry-points.
//
// The Basecamp owner app uses these to act as the owner over Logos Messaging
// (Waku), with no intermediary server: it reads the agent's approval requests
// and replies (approve / deny / reconfigure) over the same channel the agent
// holds the other end of. The agent side runs in the headless `agent` binary;
// the two talk over a Waku node, each deriving the same two topics from the
// (agent, owner) pair.
//
// Limits are passed as decimal strings because LEZ token amounts exceed 64
// bits and u128 has no portable C type. The library path can be overridden with
// LOGOS_AGENT_FFI_PATH; otherwise the loader searches the default name.
class OwnerFfiClient
{
public:
    // Open the owner channel against the Waku node at `messagingUrl` for the
    // given agent account and owner identity. An empty URL uses an in-memory
    // backend (tests only; it does not cross processes). Returns true on
    // success and fills lastError() otherwise.
    bool open(const QString& messagingUrl,
              const QString& agentAccountId,
              const QString& ownerIdentity);
    bool isOpen() const { return m_handle != nullptr; }
    void close();

    // Owner reads the agent's pending approval requests:
    // {"ok":true,"requests":[...]} or {"ok":false,"error":...}.
    QString pollRequests();
    // Approve (approve=true) or deny a pending request by id.
    QString decide(const QString& requestId, bool approve);
    // Set the per-transaction spending limit (decimal string).
    QString configureLimit(const QString& limit);
    // Set the per-period spending policy (limit decimal string, seconds u64).
    QString configurePeriod(const QString& limit, unsigned long long seconds);

    QString lastError() const { return m_lastErr; }
    ~OwnerFfiClient();

private:
    using FreeFn        = void  (*)(char*);
    using ChannelNewFn = void* (*)(const char*, const char*, const char*);
    using PollFn       = char* (*)(void*);
    using DecideFn     = char* (*)(void*, const char*, bool);
    using CfgLimitFn   = char* (*)(void*, const char*);
    using CfgPeriodFn  = char* (*)(void*, const char*, unsigned long long);
    using ChannelFreeFn = void  (*)(void*);

    bool load();
    QString errJson(const QString& message) const;

    QLibrary       m_lib;
    FreeFn         m_free        = nullptr;
    ChannelNewFn  m_channelNew  = nullptr;
    PollFn        m_poll         = nullptr;
    DecideFn      m_decide       = nullptr;
    CfgLimitFn    m_cfgLimit      = nullptr;
    CfgPeriodFn   m_cfgPeriod     = nullptr;
    ChannelFreeFn m_channelFree  = nullptr;
    void*         m_handle        = nullptr;
    bool          m_loaded        = false;
    QString       m_lastErr;
};

#endif // OWNER_FFI_CLIENT_H
