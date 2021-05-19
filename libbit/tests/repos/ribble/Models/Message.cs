using System;
using Cassandra;
using Cassandra.Mapping.Attributes;
using Cassandra.Mapping;
using System.Text.Json.Serialization;
using HotChocolate.Types.Relay;

namespace RibbleChatServer.Models
{
    public record SendMessageRequest(Guid AuthorId, string AuthorUsername, Guid GroupId, string content);

    [Table("messages")]
    public record ChatMessage
    (
        [property:PartitionKey]
        [property:Column("group_id")]
        [ID] Guid GroupId,

        [property:ClusteringKey(0, SortOrder.Descending)]
        [property:Column("time_stamp")]
        DateTimeOffset Timestamp,

        [property:Column("message_id")]
        [property:JsonPropertyName("id")]
        [ID] Guid MessageId,

        [property:Column("author_id")]
        [ID] Guid AuthorId,

        [property:Column("author_username")]
        string AuthorUsername,

        [property:Column("content")]
        string Content
    );



}
