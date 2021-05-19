using System;
using HotChocolate;
using RibbleChatServer.Utils;

namespace RibbleChatServer.GraphQL
{
    public class ErrorException : Exception
    {
        public ErrorException(string? message) : base(message)
        {
        }
    }

    /// Bad Request
    public class RequestException : Exception
    {
        public RequestException(string? message) : base(message)
        {
        }
    }

    public class GraphQLErrorFilter : IErrorFilter
    {
        public IError OnError(IError error)
        {
            return error.Exception?.Message.Map(error.WithMessage) ?? error;
        }
    }
}