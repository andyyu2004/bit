using System;
using System.Linq;
using System.Threading.Tasks;
using HotChocolate;
using HotChocolate.Subscriptions;
using HotChocolate.Types;
using Microsoft.AspNetCore.Identity;
using Microsoft.EntityFrameworkCore;
using RibbleChatServer.Data;
using RibbleChatServer.GraphQL.ResultTypes;
using RibbleChatServer.Models;

namespace RibbleChatServer.GraphQL
{
    public class MutationType : ObjectType<Mutation>
    {
    }

    public partial class Mutation
    {

        public async Task<ILoginResult> Login(
            string usernameOrEmail,
            string password,
            [Service] UserManager<User> userManager,
            [Service] SignInManager<User> signinManager,
            [Service] MainDbContext dbContext)
        {

            var user = await userManager.FindByEmailAsync(usernameOrEmail)
                ?? await userManager.FindByNameAsync(usernameOrEmail);

            if (user is null) return new LoginUnknownUserError(usernameOrEmail);

            var loginResult = await signinManager.PasswordSignInAsync(user, password, false, false);
            if (!loginResult.Succeeded) return new LoginIncorrectPasswordError();

            var loadedUser = await dbContext.Users
                .Include(user => user.Groups)
                .SingleAsync(u => u.Id == user.Id);
            return new LoginSuccess(loadedUser);
        }


        public record RegisterMutationInput(
            string Username,
            string Email,
            string Password
        );

        public record RegisterMutationPayload(User user);

        public async Task<RegisterMutationPayload> Register(
            RegisterMutationInput input,
            [Service] UserManager<User> userManager
        )
        {
            var (username, email, password) = input;
            var zxcvbnResult = Zxcvbn.Core.EvaluatePassword(password);
            if (zxcvbnResult.Score < 3)
                throw new RequestException("Password is too weak");

            var user = new User(
                UserName: username,
                Email: email
            );
            var userCreationResult = await userManager.CreateAsync(user, password);
            if (!userCreationResult.Succeeded) throw new Exception(
                userCreationResult.Errors.First().Description
            );
            // TODO does this user have its fields properly populated
            return new RegisterMutationPayload(user);

        }

        public record TestMutationInput(int x);
        public record TestMutationPayload(int y);

        public async Task<TestMutationPayload> TestMutation(
            TestMutationInput input,
            [Service] ITopicEventSender eventSender
        )
        {
            await eventSender.SendAsync(new Topic.Test(), input.x);
            return new TestMutationPayload(input.x);
        }
    }
}