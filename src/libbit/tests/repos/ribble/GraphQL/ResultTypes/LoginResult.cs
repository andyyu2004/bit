using HotChocolate.Types;
using RibbleChatServer.Models;

namespace RibbleChatServer.GraphQL.ResultTypes
{

    [UnionType("LoginResult")]
    public interface ILoginResult
    {
    }

    public class LoginSuccess : ILoginResult
    {
        public User User { get; }

        public LoginSuccess(User user) => User = user;
    }

    public class LoginUnknownUserError : ILoginResult
    {
        public string Error { get; init; }
        public LoginUnknownUserError(string usernameOrEmail) =>
            Error = $"user with username or email `{usernameOrEmail}` does not exist";
    }

    // TODO remove this distinction of unknown user/incorrect password in the future to prevent attacks
    public class LoginIncorrectPasswordError : ILoginResult
    {
        public string Error { get; init; }

        public LoginIncorrectPasswordError() => Error = $"incorrect password";
    }

}
