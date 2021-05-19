using System.Linq;
using System.Threading.Tasks;
using Microsoft.AspNetCore.Identity;
using Microsoft.AspNetCore.Mvc;
using Microsoft.EntityFrameworkCore;
using RibbleChatServer.Data;
using RibbleChatServer.Models;

[ApiController]
public class UserController : ControllerBase
{
    private readonly MainDbContext dbContext;
    private readonly UserManager<User> userManager;
    private readonly SignInManager<User> signInManager;

    public UserController(MainDbContext dbContext, UserManager<User> userManager, SignInManager<User> signInManager, IMessageDb db)
    {
        this.dbContext = dbContext;
        this.userManager = userManager;
        this.signInManager = signInManager;
    }

    [HttpGet]
    [Route("/api")]
    public IActionResult RibbleApiRoot() => Ok("Welcome to the Ribble API!");

    [HttpPost]
    [Route("/api/users")]
    public async Task<ActionResult<UserResponse>> Register([FromBody] RegisterUserInfo userInfo)
    {
        var zxcvbnResult = Zxcvbn.Core.EvaluatePassword(userInfo.Password);
        if (zxcvbnResult.Score < 3) return UnprocessableEntity("Password is too weak");

        var user = new User(
            UserName: userInfo.Username,
            Email: userInfo.Email
        );
        var userCreationResult = await userManager.CreateAsync(user, userInfo.Password);
        if (!userCreationResult.Succeeded)
            return UnprocessableEntity(userCreationResult.Errors.First());
        return Created("", user);
    }

    [HttpPost]
    [Route("/api/auth")]
    public async Task<ActionResult<UserResponse>> Login(LoginUserInfo loginInfo)
    {
        var user = await userManager.FindByEmailAsync(loginInfo.UsernameOrEmail) ?? await userManager.FindByNameAsync(loginInfo.UsernameOrEmail);

        if (user is null)
            return NotFound($"User with email or username {loginInfo.UsernameOrEmail} does not exist");

        var loginResult = await signInManager.PasswordSignInAsync(user, loginInfo.Password, false, false);
        if (!loginResult.Succeeded)
            return BadRequest("Incorrect Password");

        var loadedUser = await dbContext.Users
            .Include(user => user.Groups)
            .SingleAsync(u => u.Id == user.Id);
        return Ok((UserResponse)loadedUser);
    }
}
