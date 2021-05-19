using System;
using HotChocolate.Types;
using HotChocolate;
using System.Threading.Tasks;
using HotChocolate.Execution;
using HotChocolate.Subscriptions;
using RibbleChatServer.Models;
using System.Threading;
using HotChocolate.Types.Relay;

namespace RibbleChatServer.GraphQL
{
    public class SubscriptionType : ObjectType<Subscription>
    {
    }

    public class Subscription
    {
        [SubscribeAndResolve]
        public async ValueTask<ISourceStream<int>> OnTestEvent(
            [Service] ITopicEventReceiver eventReceiver,
            CancellationToken ct
        ) => await eventReceiver.SubscribeAsync<Topic, int>(new Topic.Test(), ct);

        [SubscribeAndResolve]
        public async ValueTask<ISourceStream<ChatMessage>> OnMessageSent(
            [Service] ITopicEventReceiver eventReceiver,
            [ID] Guid groupId,
            CancellationToken ct
        ) => await eventReceiver.SubscribeAsync<Topic, ChatMessage>(new Topic.NewMessage(groupId), ct);
    }
}
